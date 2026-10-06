// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Client authentication: every request to the ITS-REST surface and to
//! `OPTIONS {base}/` comes from a caller the gateway verified (§13.1, N25,
//! CP-17 inbound; §7a.2).
//!
//! A caller presents an RFC 9068 access token in `Authorization: Bearer`
//! (RFC 6750 §2.1). Its JOSE header names an algorithm on the allow-list
//! [`ALGORITHMS`], never `none` and never an HMAC (RFC 8725 §3.1, §3.2), and
//! the `at+jwt` type (RFC 9068 §4). Its `iss` names an issuer on the trust
//! list, whose key set verifies the signature ([`keys`]), or whose RFC 7662
//! introspection endpoint calls it active ([`fetch`]); a token that is not a
//! JWS goes to the one introspected issuer. Its `aud` names this gateway, and
//! `exp` and `nbf` hold within the configured clock skew. In the edge mode, a
//! proxy authenticates the caller and the gateway verifies the proxy's
//! signed assertion, in the header the deployment names, under the same
//! rules.
//!
//! The caller then needs what the operation requires ([`permission`]): a
//! SMART on openEHR resource scope read with `openehr-sdt`, and a purpose of
//! use unless the deployment declared it optional (§13.4). A request that
//! reaches patient data also needs a natural person behind the token, or a
//! client its issuer declares as acting for the professional the token
//! names, and the authentication assurance its issuer's entry requires
//! (Regulation (EU) 2025/327 Annex II 3.1). A request that
//! fails is answered `401`, `403` or `503` and reaches nothing behind the
//! gate; one that passes carries its [`Caller`] in its extensions. The
//! caller's credential is never forwarded to a node: the node dispatch sends
//! each endpoint its own onward credentials and copies no client header
//! without a rule naming it.
//!
//! The health family and `GET {base}/` stay open: they describe the process
//! and the product, no patient and no member (no specification governs this:
//! our own design). The JSON Web Key Set the gateway will publish for its
//! own client assertions is public too (RFC 7517), and is served beside
//! them, outside the gate.

pub mod caller;
mod claims;
pub mod fetch;
pub mod keys;
pub mod permission;
mod reference;
pub mod refusal;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use ferrofed_engine::conveyance::{Acting, AssuranceLevel};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EhrId;
use http::{HeaderMap, Method, header};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, Validation};
use openehr_its::rest::routes::{self, Lookup};
use openehr_sdt::smart_scopes::SmartScope;
use secrecy::SecretString;

use crate::ITS_REST_PREFIX;
use crate::auth::caller::{Caller, PatientContext, Stated, VerifiedBy};
use crate::auth::claims::{AccessToken, Introspected};
use crate::auth::fetch::{FetchError, Fetcher};
use crate::auth::keys::{KeyError, KeySet};
use crate::auth::permission::grant;
use crate::auth::permission::{Requirement, Resource};
use crate::auth::refusal::Refusal;
use crate::base_path::BasePath;
use crate::config::auth::{AuthMode, AuthSettings, IssuerSettings, Verification};
use crate::facade::security::TARGET;
use crate::metrics::security::Event;
use crate::request_id;
use crate::state::AppState;

/// The signature algorithms a token may be signed with (RFC 8725 §3.1,
/// §3.2): ECDSA and RSA, never `none` and never an HMAC.
pub const ALGORITHMS: [Algorithm; 4] = [
    Algorithm::ES256,
    Algorithm::ES384,
    Algorithm::PS256,
    Algorithm::RS256,
];

/// The verifier of every caller, built from `[auth]`.
#[derive(Debug)]
pub struct Gate {
    /// Where the credential travels.
    mode: AuthMode,
    /// The audience every token names.
    audience: Option<String>,
    /// The clock skew allowed on `exp` and `nbf`.
    skew: Duration,
    /// Whether a purpose of use is required.
    purpose_required: bool,
    /// The issuers on the trust list.
    issuers: Vec<Trusted>,
    /// The HTTP client of key set fetches and introspection.
    fetcher: Fetcher,
    /// The key the security log's caller references are made under.
    references: reference::References,
}

/// One issuer on the trust list, with its key set when it has one.
#[derive(Debug)]
struct Trusted {
    /// The issuer as configured.
    settings: IssuerSettings,
    /// Its key set, for an issuer whose tokens are verified by signature.
    keys: Option<KeySet>,
}

impl Gate {
    /// Returns the gate `settings` describe; no key set is read yet.
    #[must_use]
    pub fn new(settings: &AuthSettings) -> Self {
        let issuers = settings
            .issuers
            .iter()
            .map(|issuer| Trusted {
                keys: match &issuer.verification {
                    Verification::KeySet(source) => Some(KeySet::new(
                        source.clone(),
                        settings.key_set_max_age,
                        settings.key_set_refetch,
                    )),
                    Verification::Introspection(_) => None,
                },
                settings: issuer.clone(),
            })
            .collect();
        Self {
            mode: settings.mode.clone(),
            audience: settings.audience.clone(),
            skew: settings.clock_skew,
            purpose_required: settings.purpose_required,
            issuers,
            fetcher: Fetcher::new(settings.fetch_timeout),
            references: reference::References::fresh(),
        }
    }

    /// Verifies the caller of a request with `headers` and checks it against
    /// `requirement`, where `named` is the resource the request path names.
    ///
    /// The caller keeps its verified access token only when `exchanging`, a
    /// node's grant exchanging it (RFC 8693 §2.1), and only when the gateway
    /// verified it by signature or introspection; otherwise the token is
    /// dropped with the request's headers.
    ///
    /// # Errors
    /// Returns the [`Refusal`] the request is answered with.
    pub async fn admit(
        &self,
        headers: &HeaderMap,
        (requirement, named): (Requirement, Option<&str>),
        exchanging: bool,
    ) -> Result<Caller, Refusal> {
        // NOTE: no specification governs this: our own design; an operation no
        // caller may reach is refused before any credential is read.
        if requirement == Requirement::Refused {
            return Err(Refusal::Operation);
        }
        let credential = self.credential(headers)?;
        let (caller, trusted) = self.verify(credential).await?;
        let caller = caller.with_audience(self.audience.clone());
        let mut caller = if exchanging {
            caller.with_token(SecretString::from(credential.to_owned()))
        } else {
            caller
        };
        match requirement {
            Requirement::Caller => return Ok(caller),
            Requirement::Refused => return Err(Refusal::Operation),
            Requirement::Operator => {
                let scope = trusted.settings.operator_scope.as_deref();
                return grant::operator(scope, caller.granted())
                    .then_some(caller)
                    .ok_or(Refusal::Scope);
            }
            Requirement::Demographic => {
                if !trusted
                    .settings
                    .demographic_clients
                    .contains(caller.client_id())
                {
                    return Err(Refusal::Demographic);
                }
                // NOTE: SMART on openEHR master08 §Resource Scopes: a patient scope reaches "data
                // within that patient's EHR", so listing its client never widens it to parties.
                if grant::patient_grant(caller.scopes()) {
                    return Err(Refusal::PatientDemographic);
                }
            }
            Requirement::Scope {
                family,
                permission,
                resource,
            } => {
                let named = match resource {
                    Resource::Unnamed => None,
                    Resource::Path(_) => named,
                };
                let honoured = grant::Honoured {
                    backend: grant::backend(&trusted.settings.backend_clients, caller.client_id()),
                    patient: trusted.settings.patient.is_some(),
                };
                if !grant::granted(caller.scopes(), (family, permission), named, honoured) {
                    return Err(Refusal::Scope);
                }
                // NOTE: N26, RFC 8693 §2.1 scope: an exchanged token asks for the
                // granted scopes that cover the operation, never the whole grant.
                let covering: Vec<&SmartScope> = caller
                    .scopes()
                    .iter()
                    .filter(|scope| {
                        grant::granted(
                            std::slice::from_ref(*scope),
                            (family, permission),
                            named,
                            honoured,
                        )
                    })
                    .collect();
                let owned: Vec<SmartScope> =
                    covering.iter().map(|scope| (*scope).clone()).collect();
                let confined = grant::confined(&owned);
                let backend = grant::backend_only(&owned);
                let covering = SmartScope::format_all(covering);
                caller = caller.with_covering(covering);
                // NOTE: SMART on openEHR master08 §Resource Scopes: a `system/` scope is
                // granted "to backend applications acting without a user context".
                if backend {
                    caller = caller.with_acting(Acting::Client);
                }
                if confined {
                    let context = patient_context(&caller, trusted)?;
                    caller = caller.with_patient(context);
                }
            }
        }
        if requirement.reaches_patient_data() {
            professional(&caller, &trusted.settings)?;
        }
        // NOTE: §13.4 authn-purpose-of-use, a node must never be left to infer
        // the purpose, so a request that reaches past the gate carries one.
        if self.purpose_required && caller.purposes().is_empty() {
            return Err(Refusal::PurposeOfUse);
        }
        Ok(caller)
    }

    /// The credential of a request: the bearer token, or the edge's
    /// assertion, sent exactly once.
    fn credential<'h>(&self, headers: &'h HeaderMap) -> Result<&'h str, Refusal> {
        let name = match &self.mode {
            AuthMode::Token => &header::AUTHORIZATION,
            AuthMode::Edge(name) => name,
        };
        let mut values = headers.get_all(name).iter();
        let (Some(value), None) = (values.next(), values.next()) else {
            return Err(if headers.contains_key(name) {
                Refusal::Malformed
            } else {
                Refusal::Missing
            });
        };
        let text = value.to_str().map_err(|_opaque| Refusal::Malformed)?;
        let credential = match self.mode {
            AuthMode::Token => {
                // NOTE: RFC 9110 §11.1, the auth-scheme is case-insensitive, and RFC
                // 6750 §2.1 puts one b64token after it.
                let (scheme, token) = text.split_once(' ').ok_or(Refusal::Malformed)?;
                if !scheme.eq_ignore_ascii_case("bearer") {
                    return Err(Refusal::Malformed);
                }
                token.trim_start_matches(' ')
            }
            AuthMode::Edge(_) => text,
        };
        if credential.is_empty() {
            return Err(Refusal::Malformed);
        }
        Ok(credential)
    }

    /// Verifies `credential` and returns its caller and the issuer that
    /// vouched for it.
    async fn verify(&self, credential: &str) -> Result<(Caller, &Trusted), Refusal> {
        let edge = matches!(self.mode, AuthMode::Edge(_));
        if credential.split('.').count() != 3 {
            if edge {
                return Err(Refusal::Malformed);
            }
            let trusted = self
                .issuers
                .iter()
                .find(|issuer| {
                    matches!(issuer.settings.verification, Verification::Introspection(_))
                })
                .ok_or(Refusal::Issuer)?;
            return Ok((self.introspect(trusted, credential).await?, trusted));
        }
        let header =
            jsonwebtoken::decode_header(credential).map_err(|_unreadable| Refusal::Malformed)?;
        if !ALGORITHMS.contains(&header.alg) {
            return Err(Refusal::Algorithm);
        }
        let typed = header.typ.as_deref().is_some_and(|typ| {
            typ.eq_ignore_ascii_case("at+jwt") || typ.eq_ignore_ascii_case("application/at+jwt")
        });
        if !typed {
            return Err(Refusal::Type);
        }
        // NOTE: RFC 8725 §3.10, the unverified `iss` only picks the issuer whose
        // keys then verify the token, `iss` included.
        let named: Issued = jsonwebtoken::dangerous::insecure_decode_claims(credential)
            .map_err(|_unreadable| Refusal::Malformed)?;
        let trusted = self
            .issuers
            .iter()
            .find(|issuer| issuer.settings.issuer == named.iss)
            .ok_or(Refusal::Issuer)?;
        let Some(keys) = &trusted.keys else {
            return Ok((self.introspect(trusted, credential).await?, trusted));
        };
        let key = keys
            .key(header.kid.as_deref(), header.alg, &self.fetcher)
            .await
            .map_err(|error| match error {
                KeyError::Unavailable(source) => {
                    Event::KeySetUnavailable.record();
                    tracing::warn!(
                        target: TARGET,
                        event = "key-set-unavailable",
                        issuer = trusted.settings.issuer,
                        error = crate::chain(&source),
                        "an issuer's key set could not be had, so its tokens cannot be verified"
                    );
                    Refusal::Unavailable
                }
                KeyError::Unknown => Refusal::Key,
                KeyError::Unusable => Refusal::Signature,
            })?;
        let audience = self.audience.as_deref().ok_or(Refusal::Audience)?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = vec![header.alg];
        validation.leeway = self.skew.as_secs();
        validation.validate_nbf = true;
        validation.set_audience(&[audience]);
        validation.set_issuer(&[trusted.settings.issuer.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let token = jsonwebtoken::decode::<AccessToken>(credential, &key, &validation)
            .map_err(|error| refusal_of(error.kind()))?;
        let verified_by = if edge {
            VerifiedBy::Edge
        } else {
            VerifiedBy::Signature
        };
        let launch_ehr_id = token.claims.launch_ehr_id();
        let requester = token.claims.requester(trusted.settings.requester.as_ref());
        let professional = token.claims.professional();
        let assurance = assurance_of(&trusted.settings, |claim| token.claims.text(claim));
        let stated = token.claims.stated();
        let acting = acting(&stated.subject, &stated.client_id);
        let caller = Caller::new(stated, verified_by)
            .with_launch_ehr_id(launch_ehr_id)
            .with_requester(requester)
            .with_professional(professional)
            .with_assurance(assurance)
            .with_acting(acting);
        Ok((caller, trusted))
    }

    /// Asks `trusted`'s introspection endpoint about `token` and reads its
    /// answer as RFC 7662 §2.2 states it.
    async fn introspect(&self, trusted: &Trusted, token: &str) -> Result<Caller, Refusal> {
        let Verification::Introspection(endpoint) = &trusted.settings.verification else {
            return Err(Refusal::Issuer);
        };
        let bytes = self
            .fetcher
            .introspect(endpoint, token)
            .await
            .and_then(|bytes| {
                serde_json::from_slice::<Introspected>(&bytes).map_err(FetchError::Malformed)
            });
        let answer = match bytes {
            Ok(answer) => answer,
            Err(source) => {
                Event::IntrospectionUnavailable.record();
                tracing::warn!(
                    target: TARGET,
                    event = "introspection-unavailable",
                    issuer = trusted.settings.issuer,
                    error = crate::chain(&source),
                    "an issuer's introspection endpoint did not answer, so its tokens cannot be verified"
                );
                return Err(Refusal::Unavailable);
            }
        };
        if !answer.active {
            return Err(Refusal::Inactive);
        }
        if answer
            .iss
            .as_deref()
            .is_some_and(|iss| iss != trusted.settings.issuer)
        {
            return Err(Refusal::Issuer);
        }
        let audience = self.audience.as_deref().ok_or(Refusal::Audience)?;
        if !answer.aud.as_ref().is_some_and(|aud| aud.names(audience)) {
            return Err(Refusal::Audience);
        }
        let now = jiff::Timestamp::now().as_second();
        let skew = i64::try_from(self.skew.as_secs()).unwrap_or(i64::MAX);
        let exp = answer.exp.ok_or(Refusal::Malformed)?;
        if exp.saturating_add(skew) < now {
            return Err(Refusal::Expired);
        }
        if answer.nbf.is_some_and(|nbf| nbf.saturating_sub(skew) > now) {
            return Err(Refusal::NotYetValid);
        }
        let (Some(subject), Some(client_id)) = (answer.sub, answer.client_id) else {
            return Err(Refusal::Malformed);
        };
        let acting = acting(&subject, &client_id);
        let stated = Stated {
            issuer: trusted.settings.issuer.clone(),
            subject,
            client_id,
            organisation: answer.declared.organisation(),
            granted: answer.scope.unwrap_or_default(),
            purposes: answer.declared.purposes(),
        };
        let requester = trusted
            .settings
            .requester
            .as_ref()
            .and_then(|named| answer.others.requester(named));
        let assurance = assurance_of(&trusted.settings, |claim| answer.others.text(claim));
        Ok(Caller::new(stated, VerifiedBy::Introspection)
            .with_launch_ehr_id(answer.ehr_id)
            .with_requester(requester)
            .with_professional(answer.declared.professional())
            .with_assurance(assurance)
            .with_acting(acting))
    }

    /// Whether any caller can be admitted: some issuer is on the trust list.
    #[must_use]
    pub fn admits_any(&self) -> bool {
        !self.issuers.is_empty()
    }
}

/// The patient `caller`'s `patient/` grant is confined to: its token's
/// `ehrId` at the member `trusted` is bound to.
///
/// # Errors
///
/// Returns [`Refusal::PatientContext`] when the token carries no `ehrId`, or
/// one that is no openEHR `HIER_OBJECT_ID`, and [`Refusal::Scope`] when the
/// issuer is bound to no member, whose `patient/` grants then count for
/// nothing.
fn patient_context(caller: &Caller, trusted: &Trusted) -> Result<PatientContext, Refusal> {
    let binding = trusted.settings.patient.as_ref().ok_or(Refusal::Scope)?;
    // NOTE: SMART on openEHR master04 §Capabilities conveys the context "via the `ehrId`
    // token claim"; one absent, or no HIER_OBJECT_ID, names no patient to confine to.
    let ehr_id = caller
        .launch_ehr_id()
        .and_then(|claim| EhrId::new(claim).ok())
        .ok_or(Refusal::PatientContext)?;
    Ok(PatientContext::new(
        binding.endpoint.clone(),
        binding.ehr_id_system.clone(),
        ehr_id,
    ))
}

/// Holds `caller`, admitted to patient data, to who acts and how they
/// authenticated, as `issuer`'s entry requires.
///
/// Regulation (EU) 2025/327 Annex II 3.1 asks for "reliable mechanisms for
/// the identification and authentication of health professionals", and
/// Implementing Regulation (EU) 2026/2099 Art 6(3) sets the assurance level
/// of that authentication for a cross-border exchange. Which claim values
/// stand for which level, and the least level, are the deployment's per
/// issuer: no specification governs this: our own design.
///
/// # Errors
///
/// Returns [`Refusal::NaturalPerson`] when a client acts and the issuer does
/// not declare its client tokens as acting for the professional they name,
/// or the token names none, and [`Refusal::Assurance`] when the issuer
/// requires a level and the token states none at or above it.
fn professional(caller: &Caller, issuer: &IssuerSettings) -> Result<(), Refusal> {
    if caller.acting() == Acting::Client
        && !(issuer.client_tokens_act_for_professional && caller.names_professional())
    {
        return Err(Refusal::NaturalPerson);
    }
    if let Some(assurance) = &issuer.assurance
        && caller
            .assurance()
            .is_none_or(|level| level < assurance.minimum)
    {
        return Err(Refusal::Assurance);
    }
    Ok(())
}

/// The assurance level `stated`, the value of the claim `issuer`'s entry
/// names, stands for, when the entry declares a mapping and the value is in
/// it.
fn assurance_of(
    issuer: &IssuerSettings,
    stated: impl Fn(&str) -> Option<String>,
) -> Option<AssuranceLevel> {
    let assurance = issuer.assurance.as_ref()?;
    // NOTE: no specification governs this: our own design; a value the entry declares at
    // no level states no level, which no minimum admits.
    stated(&assurance.claim).and_then(|value| assurance.level_of(&value))
}

/// Who acts behind a token whose `sub` is `subject` and whose `client_id`
/// is `client_id`.
fn acting(subject: &str, client_id: &str) -> Acting {
    // NOTE: IHE IUA ITI TF-2 3.71.4.2.2.1: `sub` is the "unique identifier of the
    // user" if known, "the client_id otherwise".
    if subject == client_id {
        Acting::Client
    } else {
        Acting::Person
    }
}

/// The `iss` of a token not yet verified.
#[derive(Debug, serde::Deserialize)]
struct Issued {
    /// `iss`.
    iss: String,
}

/// The refusal a `jsonwebtoken` verification failure stands for.
fn refusal_of(kind: &ErrorKind) -> Refusal {
    match kind {
        ErrorKind::ExpiredSignature => Refusal::Expired,
        ErrorKind::ImmatureSignature => Refusal::NotYetValid,
        ErrorKind::InvalidAudience => Refusal::Audience,
        ErrorKind::InvalidIssuer => Refusal::Issuer,
        ErrorKind::InvalidAlgorithm | ErrorKind::MissingAlgorithm => Refusal::Algorithm,
        ErrorKind::InvalidSignature
        | ErrorKind::InvalidEcdsaKey
        | ErrorKind::InvalidRsaKey(_)
        | ErrorKind::InvalidKeyFormat => Refusal::Signature,
        _ => Refusal::Malformed,
    }
}

/// The gate and where it stands: the middleware's state.
#[derive(Debug)]
pub struct Guard {
    /// The verifier.
    gate: Gate,
    /// The base path every guarded route sits under.
    base: BasePath,
    /// The state whose running federation says whether any node exchanges
    /// a caller's token.
    state: Arc<AppState>,
}

impl Guard {
    /// Returns the guard of the surface under `base`, verifying with `gate`,
    /// keeping a caller's token only while `state`'s federation exchanges
    /// tokens at some node.
    #[must_use]
    pub fn new(gate: Gate, base: BasePath, state: Arc<AppState>) -> Self {
        Self { gate, base, state }
    }

    /// Whether the running federation exchanges a caller's token at any
    /// node (RFC 8693), so the gate keeps the token it verified.
    fn exchanging(&self) -> bool {
        self.state
            .federation()
            .is_some_and(|federation| federation.clients().exchanging())
    }

    /// What a request of `method` to `path` requires, with the resource its
    /// path names, or `None` for a path outside the gate.
    #[must_use]
    pub fn requirement(
        &self,
        method: &Method,
        path: &str,
    ) -> Option<(Requirement, Option<String>)> {
        let root = path == self.base.as_str() || path == self.base.join("/");
        if root && method == Method::OPTIONS {
            return Some((Requirement::Caller, None));
        }
        if crate::operator::addresses(&self.base, path) {
            return Some((Requirement::Operator, None));
        }
        let prefix = self.base.join(ITS_REST_PREFIX.trim_end_matches('/'));
        let relative = path
            .strip_prefix(prefix.as_str())
            .filter(|relative| relative.starts_with('/'))?;
        if method == Method::OPTIONS {
            return Some((Requirement::Caller, None));
        }
        let Lookup::Matched(matched) = routes::lookup(method, relative) else {
            return Some((Requirement::Caller, None));
        };
        // NOTE: no specification governs this: our own design; the table lists
        // every operation (a test holds it so), and an unlisted one is refused.
        let requirement = permission::of(&matched).unwrap_or(Requirement::Refused);
        let named = match requirement {
            Requirement::Scope {
                resource: Resource::Path(name),
                ..
            } => matched
                .path_param(name)
                .and_then(|param| param.decoded().ok()),
            _ => None,
        };
        Some((requirement, named))
    }
}

/// The middleware every route of the gateway passes: a request inside the
/// gate is admitted with its [`Caller`] in its extensions, or refused.
pub async fn guard(State(guard): State<Arc<Guard>>, mut request: Request, next: Next) -> Response {
    let path = request
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| request.uri().path(), |original| original.path())
        .to_owned();
    let Some((requirement, named)) = guard.requirement(request.method(), &path) else {
        return next.run(request).await;
    };
    let outbound = request_id::outbound(request.extensions()).map(|id: OutboundId| id.to_string());
    match guard
        .gate
        .admit(
            request.headers(),
            (requirement, named.as_deref()),
            guard.exchanging(),
        )
        .await
    {
        Ok(caller) => {
            if caller.verified_by() == VerifiedBy::Edge {
                guard
                    .gate
                    .references
                    .edge_asserted(&caller, outbound.as_deref());
            }
            request.extensions_mut().insert(caller);
            next.run(request).await
        }
        Err(refusal) => {
            crate::metrics::security::refused(refusal);
            tracing::warn!(
                target: TARGET,
                event = "caller-refused",
                reason = refusal.reason(),
                status = refusal.code().status().as_u16(),
                request_id = outbound,
                "a request was refused at client authentication"
            );
            let request_id = request_id::of(request.headers()).unwrap_or_default();
            refusal.response(request_id)
        }
    }
}
