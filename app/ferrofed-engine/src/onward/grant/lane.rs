// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward lane of one endpoint (§13.1, N25).
//!
//! Every call a verified caller originates reaches the node under the token
//! `ferrohealth-smart`'s onward client obtains for that caller (RFC 8693
//! §2.1, RFC 7523 §2.1).
//!
//! The lane's issuer signs tokens of one kind, and a caller reaches the node
//! through it only when that kind carries the caller ([`route`]): a person of
//! the node's organisation at a `persons` issuer, a professional of another
//! party at a `federation` issuer, another party's organisation at a
//! `federation` issuer whose lane opts in ([`Organisations`]), and a patient
//! at a `patients` issuer. Every other caller is refused before anything is
//! sent. An organisation reaches the node read-only ([`reads`]), and a lane
//! with a correlation claim refuses a caller whose token does not carry it
//! before the token endpoint is asked ([`carries_claim`]). The client
//! credentials grant of the lane's own `oauth` table carries only the
//! gateway's own work, which no caller originates, and never stands in for a
//! failed exchange.
//!
//! The library runs the grant the issuer's profile picks, checks every
//! issued token (the issuer, the audience, the lifetime, the actor and the
//! person) and caches it under the caller's token and the target. Which
//! caller is exchanged at which issuer kind is our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_registry::id::EndpointId;
use ferrohealth_smart::client::onward::exchange::Onward;
use ferrohealth_smart::client::onward::{OnwardError, SubjectToken as OnwardSubject, Target};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use http::Method;
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider};
use openehr_its::rest::routes::RouteMatch;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::value::RawValue;

use crate::conveyance::Conveyance;
use crate::dispatch::SharedCredentials;
use crate::hygiene::Withheld;
use crate::onward::grant::exchange::{ForCall, OnBehalf, SubjectToken, carries_withheld};

/// The scheme of an RFC 6750 §2.1 bearer credential.
const BEARER: &str = "bearer";

/// How a caller of one kind reaches a node through a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Route {
    /// Under a token the lane's issuer exchanges the caller's for.
    Exchanged,
    /// Not at all: the lane's issuer carries no caller of this kind.
    Refused,
}

/// Whether a lane carries another party's organisation, which no named
/// person stands behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Organisations {
    /// The lane refuses an organisation, the default.
    Refused,
    /// The lane exchanges an organisation's token, and the node is reached
    /// read-only; the deployment opts in per agreement.
    ReadOnly,
}

/// Returns how a caller of `caller` reaches a node whose lane's issuer signs
/// tokens of `issuer`, carrying `organisations`.
///
/// A person of the node's organisation, acting alone, through a client or
/// for another, is exchanged at a `persons` issuer, a professional of
/// another party at a `federation` issuer, and a patient at a `patients`
/// issuer. Another party's organisation is exchanged at a `federation`
/// issuer whose lane opts in. A system and a guest are carried by none, so
/// they are refused.
// NOTE: no specification governs this: our own design; ferrohealth-smart classifies every
// contact-point token as a person, so a contact point never reaches the organisation arm.
#[must_use]
pub const fn route(caller: &CallerKind, issuer: IssuerKind, organisations: Organisations) -> Route {
    match (caller, issuer) {
        (
            CallerKind::Person | CallerKind::Delegated { .. } | CallerKind::Representative { .. },
            IssuerKind::Persons,
        )
        | (CallerKind::ExternalPerson(_), IssuerKind::Federation)
        | (CallerKind::Patient, IssuerKind::Patients) => Route::Exchanged,
        (CallerKind::ExternalOrganisation(_), IssuerKind::Federation)
            if matches!(organisations, Organisations::ReadOnly) =>
        {
            Route::Exchanged
        }
        _ => Route::Refused,
    }
}

/// Returns whether the ITS-REST operation `operation` only reads: a query,
/// or a `GET`, `HEAD` or `OPTIONS` request.
// NOTE: RFC 9110 §9.2.1, these methods are safe; every operation of the ITS-REST query
// group runs AQL, which reads.
#[must_use]
pub fn reads(operation: &RouteMatch) -> bool {
    operation.group == "query"
        || [Method::GET, Method::HEAD, Method::OPTIONS].contains(&operation.method)
}

/// Returns whether `token`, a compact JWS, carries a value other than
/// `null` at the dotted claim path `claim`.
///
/// An opaque token carries nothing the gateway can read, so it never
/// carries the claim.
#[must_use]
pub fn carries_claim(token: &str, claim: &str) -> bool {
    // NOTE: RFC 7515 §7.1, the payload is the second of three segments, base64url.
    let mut segments = token.split('.');
    let (Some(_), Some(payload), Some(_), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };
    let Ok(payload) = URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(mut object) = serde_json::from_slice::<BTreeMap<String, Box<RawValue>>>(&payload) else {
        return false;
    };
    let mut steps = claim.split('.').peekable();
    while let Some(step) = steps.next() {
        let Some(value) = object.remove(step) else {
            return false;
        };
        if steps.peek().is_none() {
            return value.get() != "null";
        }
        let Ok(inner) = serde_json::from_str(value.get()) else {
            return false;
        };
        object = inner;
    }
    false
}

/// Why a call made for a caller got no onward token, so nothing was sent.
///
/// No variant carries a token, a secret or a claim value.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LaneError {
    /// The caller was asserted by the edge, so the gateway holds no token
    /// of it to exchange.
    #[error("the caller was asserted by the edge, so the gateway holds no token of it to exchange")]
    Edge,
    /// The call conveys a verified caller but carries no token of it.
    #[error("the call carries no verified token of its caller to exchange")]
    NoSubject,
    /// The caller is of a kind the lane's issuer does not carry.
    #[error("a {caller} caller is never exchanged at a {issuer} issuer")]
    NotCarried {
        /// The caller's kind.
        caller: &'static str,
        /// The issuer's kind.
        issuer: IssuerKind,
    },
    /// The lane carries the correlation claim of a chained route and the
    /// caller's token, opaque or a JWT without it, does not carry it, so it
    /// was not sent to the token endpoint.
    #[error("the caller's token carries no {claim} correlation claim, which the lane requires")]
    NoCorrelation {
        /// The configured claim path; never its value.
        claim: String,
    },
    /// The caller's token carries an identifier the request withholds, so
    /// it was not sent to the token endpoint (§5.4.1, N33).
    #[error("the caller's token carries a withheld patient identifier, so it was not exchanged")]
    Withheld,
    /// The issuer issued a token that is no bearer token, which the node
    /// client cannot send.
    #[error("the onward issuer issued a token the node client cannot send as a bearer token")]
    NotBearer,
    /// The exchange was refused, or its token failed the onward check.
    #[error("the onward exchange failed")]
    Onward(#[source] OnwardError),
}

/// A write through an onward lane that carries its caller read-only, such
/// as another party's organisation; nothing was sent and no token was
/// asked for.
#[derive(Debug, thiserror::Error)]
#[error("endpoint {endpoint} is reached read-only for this caller, so the write was not sent")]
pub struct ReadOnly {
    /// The endpoint.
    pub endpoint: EndpointId,
}

/// The onward lane of one endpoint.
///
/// `Debug` names the endpoint and the issuer's kind, never a token.
#[derive(Clone)]
pub struct Lane(Arc<Inner>);

/// What every call of one endpoint's lane shares.
struct Inner {
    endpoint: EndpointId,
    onward: Onward,
    target: Target,
    kind: IssuerKind,
    organisations: Organisations,
    gateway: SharedCredentials,
}

impl Lane {
    /// The lane of `endpoint`: `onward`, the client of an issuer of `kind`
    /// carrying `organisations`, asking for `target`, with `gateway` the
    /// credentials of the gateway's own requests, which no caller
    /// originates.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        (onward, target): (Onward, Target),
        (kind, organisations): (IssuerKind, Organisations),
        gateway: SharedCredentials,
    ) -> Self {
        Self(Arc::new(Inner {
            endpoint,
            onward,
            target,
            kind,
            organisations,
            gateway,
        }))
    }

    /// The kind of tokens the lane's issuer signs.
    #[must_use]
    pub fn kind(&self) -> IssuerKind {
        self.0.kind
    }
}

impl OnBehalf for Lane {
    fn provider(&self, conveyance: &Conveyance, withheld: &Arc<Withheld>) -> SharedCredentials {
        match ForCall::of(conveyance, withheld) {
            None => Arc::clone(&self.0.gateway),
            Some(call) => Arc::new(ForCaller {
                inner: Arc::clone(&self.0),
                call,
            }),
        }
    }

    fn committer_kind(&self) -> Option<IssuerKind> {
        Some(self.0.kind)
    }

    fn read_only(&self, conveyance: &Conveyance) -> bool {
        conveyance
            .subject()
            .and_then(SubjectToken::onward)
            .is_some_and(|onward| matches!(onward.kind(), CallerKind::ExternalOrganisation(_)))
    }
}

impl fmt::Debug for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lane")
            .field("endpoint", &self.0.endpoint)
            .field("kind", &self.0.kind)
            .field("organisations", &self.0.organisations)
            .finish_non_exhaustive()
    }
}

/// The credentials of one call made for a verified caller.
struct ForCaller {
    inner: Arc<Inner>,
    call: ForCall,
}

impl ForCaller {
    /// The subject the lane exchanges, or why it exchanges none.
    fn subject(&self) -> Result<&OnwardSubject, LaneError> {
        if self.call.edge {
            return Err(LaneError::Edge);
        }
        let subject = self.call.subject.as_ref().ok_or(LaneError::NoSubject)?;
        let onward = subject.onward().ok_or(LaneError::NoSubject)?;
        if route(onward.kind(), self.inner.kind, self.inner.organisations) == Route::Refused {
            return Err(LaneError::NotCarried {
                caller: onward.kind().as_str(),
                issuer: self.inner.kind,
            });
        }
        if carries_withheld(subject, &self.call.withheld) {
            return Err(LaneError::Withheld);
        }
        if let Some(claim) = self.inner.target.correlation_claim()
            && !carries_claim(subject.token().expose_secret(), claim)
        {
            return Err(LaneError::NoCorrelation {
                claim: claim.to_owned(),
            });
        }
        Ok(onward)
    }

    /// The bearer token the lane's issuer exchanges the caller's for.
    async fn token(&self) -> Result<SecretString, LaneError> {
        let subject = self.subject()?;
        let issued = self
            .inner
            .onward
            .token(subject, None, &self.inner.target)
            .await
            .map_err(LaneError::Onward)?;
        tracing::debug!(
            endpoint = %self.inner.endpoint,
            "an onward token was obtained for a caller"
        );
        issued
            .header()
            .to_str()
            .ok()
            .and_then(|value| credential(value, BEARER))
            .map(|token| SecretString::from(token.to_owned()))
            .ok_or(LaneError::NotBearer)
    }
}

#[async_trait::async_trait]
impl CredentialsProvider for ForCaller {
    async fn credentials(&self) -> Result<Credentials, CredentialsError> {
        let token = self.token().await.map_err(CredentialsError::new)?;
        Ok(Credentials::Bearer(token))
    }

    // NOTE: no specification governs this: our own design; the library checks a cached
    // token again on every use, and a 401 is never renewed with the gateway's own.
    fn refused(&self) {}
}

impl fmt::Debug for ForCaller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ForCaller")
            .field("endpoint", &self.inner.endpoint)
            .field("edge", &self.call.edge)
            .finish_non_exhaustive()
    }
}

/// Returns the credential of `value`, an `Authorization` field value, when
/// its scheme is `scheme`, which RFC 9110 §11.1 makes case-insensitive.
fn credential<'a>(value: &'a str, scheme: &str) -> Option<&'a str> {
    let (named, token) = value.split_once(' ')?;
    let token = token.trim();
    (named.eq_ignore_ascii_case(scheme) && !token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::{Organisations, Route, carries_claim, credential, reads, route};
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use ferrohealth_smart::federation::External;
    use ferrohealth_smart::kind::{CallerKind, IssuerKind};
    use http::Method;
    use openehr_its::rest::routes::{self, Lookup};

    const REFUSED: Organisations = Organisations::Refused;
    const READ_ONLY: Organisations = Organisations::ReadOnly;

    #[test]
    fn a_person_is_exchanged_at_a_persons_issuer_alone() {
        let delegated = CallerKind::Delegated { actor: None };
        for kind in [&CallerKind::Person, &delegated] {
            assert_eq!(
                Route::Exchanged,
                route(kind, IssuerKind::Persons, READ_ONLY)
            );
            assert_eq!(
                Route::Refused,
                route(kind, IssuerKind::Federation, READ_ONLY)
            );
            assert_eq!(Route::Refused, route(kind, IssuerKind::Patients, READ_ONLY));
        }
    }

    #[test]
    fn a_system_is_never_exchanged_and_never_sent_under_the_gateway() {
        for issuer in [
            IssuerKind::Persons,
            IssuerKind::Federation,
            IssuerKind::Patients,
        ] {
            assert_eq!(
                Route::Refused,
                route(&CallerKind::System, issuer, READ_ONLY)
            );
        }
    }

    #[test]
    fn a_patient_is_exchanged_at_a_patients_issuer_alone() {
        assert_eq!(
            Route::Exchanged,
            route(&CallerKind::Patient, IssuerKind::Patients, REFUSED)
        );
        assert_eq!(
            Route::Refused,
            route(&CallerKind::Patient, IssuerKind::Persons, REFUSED)
        );
    }

    #[test]
    fn the_scheme_reads_case_insensitively() {
        assert_eq!(Some("abc"), credential("Bearer abc", "bearer"));
        assert_eq!(Some("abc"), credential("bEaReR abc", "bearer"));
        assert_eq!(None, credential("DPoP abc", "bearer"));
        assert_eq!(None, credential("Bearer ", "bearer"));
    }

    fn organisation() -> CallerKind {
        CallerKind::ExternalOrganisation(Box::new(External::new(
            "synthetic-remote-organisation",
            std::collections::BTreeMap::new(),
        )))
    }

    #[test]
    fn an_organisation_is_exchanged_at_a_federation_issuer_only_when_the_lane_opts_in() {
        let organisation = organisation();
        assert_eq!(
            Route::Refused,
            route(&organisation, IssuerKind::Federation, REFUSED)
        );
        assert_eq!(
            Route::Exchanged,
            route(&organisation, IssuerKind::Federation, READ_ONLY)
        );
        for issuer in [IssuerKind::Persons, IssuerKind::Patients] {
            assert_eq!(Route::Refused, route(&organisation, issuer, READ_ONLY));
        }
    }

    #[test]
    fn opting_in_to_organisations_changes_no_other_callers_route() {
        let person = CallerKind::ExternalPerson(Box::new(External::new(
            "synthetic-remote-professional",
            std::collections::BTreeMap::new(),
        )));
        for kind in [&person, &CallerKind::System, &CallerKind::Person] {
            assert_eq!(
                route(kind, IssuerKind::Federation, REFUSED),
                route(kind, IssuerKind::Federation, READ_ONLY)
            );
        }
    }

    #[test]
    fn a_query_and_a_safe_method_read_and_every_other_operation_writes() {
        let operation = |method: Method, path: &str| match routes::lookup(&method, path) {
            Lookup::Matched(matched) => matched,
            _ => panic!("{method} {path} is an ITS-REST operation"),
        };
        let ehr = "7d44b88c-4199-4bad-97dc-d78268e01398";
        assert!(reads(&operation(Method::POST, "/query/aql")));
        assert!(reads(&operation(Method::GET, "/query/aql")));
        assert!(reads(&operation(Method::GET, &format!("/ehr/{ehr}"))));
        assert!(!reads(&operation(
            Method::POST,
            &format!("/ehr/{ehr}/contribution")
        )));
        assert!(!reads(&operation(
            Method::POST,
            &format!("/ehr/{ehr}/composition")
        )));
        assert!(!reads(&operation(Method::POST, "/ehr")));
    }

    fn jws(payload: &str) -> String {
        format!("e30.{}.c2ln", URL_SAFE_NO_PAD.encode(payload))
    }

    #[test]
    fn a_claim_is_read_from_a_jws_payload_along_its_dotted_path() {
        let token = jws(r#"{"txn":"c-1","ctx":{"log":{"id":"c-2"}},"empty":null}"#);
        assert!(carries_claim(&token, "txn"));
        assert!(carries_claim(&token, "ctx.log.id"));
        assert!(carries_claim(&token, "ctx.log"));
        assert!(!carries_claim(&token, "empty"));
        assert!(!carries_claim(&token, "ctx.log.other"));
        assert!(!carries_claim(&token, "txn.id"));
        assert!(!carries_claim(&token, "absent"));
    }

    #[test]
    fn an_opaque_or_malformed_token_carries_no_claim() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"txn":"c-1"}"#);
        for token in [
            String::from("synthetic-opaque-token"),
            format!("e30.{payload}"),
            format!("e30.{payload}.c2ln.extra"),
            String::from("e30.%%%.c2ln"),
            jws("[1]"),
        ] {
            assert!(!carries_claim(&token, "txn"), "{token}");
        }
    }
}
