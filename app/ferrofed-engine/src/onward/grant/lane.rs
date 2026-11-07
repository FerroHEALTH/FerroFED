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
//! party at a `federation` issuer, and a patient at a `patients` issuer.
//! Every other caller is refused before anything is sent. The client
//! credentials grant of the lane's own `oauth` table carries only the
//! gateway's own work, which no caller originates, and never stands in for a
//! failed exchange.
//!
//! The library runs the grant the issuer's profile picks, checks every
//! issued token (the issuer, the audience, the lifetime, the actor and the
//! person) and caches it under the caller's token and the target. Which
//! caller is exchanged at which issuer kind is our own design.

use std::fmt;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use ferrohealth_smart::client::onward::exchange::Onward;
use ferrohealth_smart::client::onward::{OnwardError, SubjectToken as OnwardSubject, Target};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use openehr_its::rest::client::{Credentials, CredentialsError, CredentialsProvider};
use secrecy::SecretString;

use crate::conveyance::Conveyance;
use crate::dispatch::SharedCredentials;
use crate::hygiene::Withheld;
use crate::onward::grant::exchange::{ForCall, OnBehalf, carries_withheld};

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

/// Returns how a caller of `caller` reaches a node whose lane's issuer signs
/// tokens of `issuer`.
///
/// A person of the node's organisation, acting alone, through a client or
/// for another, is exchanged at a `persons` issuer, a professional of
/// another party at a `federation` issuer, and a patient at a `patients`
/// issuer. A system, another party's organisation and a guest are carried
/// by none, so they are refused.
#[must_use]
pub const fn route(caller: &CallerKind, issuer: IssuerKind) -> Route {
    match (caller, issuer) {
        (
            CallerKind::Person | CallerKind::Delegated { .. } | CallerKind::Representative { .. },
            IssuerKind::Persons,
        )
        | (CallerKind::ExternalPerson(_), IssuerKind::Federation)
        | (CallerKind::Patient, IssuerKind::Patients) => Route::Exchanged,
        _ => Route::Refused,
    }
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
    gateway: SharedCredentials,
}

impl Lane {
    /// The lane of `endpoint`: `onward`, the client of an issuer of `kind`,
    /// asking for `target`, with `gateway` the credentials of the gateway's
    /// own requests, which no caller originates.
    #[must_use]
    pub fn new(
        endpoint: EndpointId,
        (onward, target): (Onward, Target),
        kind: IssuerKind,
        gateway: SharedCredentials,
    ) -> Self {
        Self(Arc::new(Inner {
            endpoint,
            onward,
            target,
            kind,
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
}

impl fmt::Debug for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lane")
            .field("endpoint", &self.0.endpoint)
            .field("kind", &self.0.kind)
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
        if route(onward.kind(), self.inner.kind) == Route::Refused {
            return Err(LaneError::NotCarried {
                caller: onward.kind().as_str(),
                issuer: self.inner.kind,
            });
        }
        if carries_withheld(subject, &self.call.withheld) {
            return Err(LaneError::Withheld);
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
    use super::{Route, credential, route};
    use ferrohealth_smart::kind::{CallerKind, IssuerKind};

    #[test]
    fn a_person_is_exchanged_at_a_persons_issuer_alone() {
        let delegated = CallerKind::Delegated { actor: None };
        for kind in [&CallerKind::Person, &delegated] {
            assert_eq!(Route::Exchanged, route(kind, IssuerKind::Persons));
            assert_eq!(Route::Refused, route(kind, IssuerKind::Federation));
            assert_eq!(Route::Refused, route(kind, IssuerKind::Patients));
        }
    }

    #[test]
    fn a_system_is_never_exchanged_and_never_sent_under_the_gateway() {
        for issuer in [
            IssuerKind::Persons,
            IssuerKind::Federation,
            IssuerKind::Patients,
        ] {
            assert_eq!(Route::Refused, route(&CallerKind::System, issuer));
        }
    }

    #[test]
    fn a_patient_is_exchanged_at_a_patients_issuer_alone() {
        assert_eq!(
            Route::Exchanged,
            route(&CallerKind::Patient, IssuerKind::Patients)
        );
        assert_eq!(
            Route::Refused,
            route(&CallerKind::Patient, IssuerKind::Persons)
        );
    }

    #[test]
    fn the_scheme_reads_case_insensitively() {
        assert_eq!(Some("abc"), credential("Bearer abc", "bearer"));
        assert_eq!(Some("abc"), credential("bEaReR abc", "bearer"));
        assert_eq!(None, credential("DPoP abc", "bearer"));
        assert_eq!(None, credential("Bearer ", "bearer"));
    }
}
