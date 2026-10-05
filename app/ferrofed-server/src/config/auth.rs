// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth]`: how a caller authenticates to the gateway (§13.1, N25), as
//! written and as resolved.
//!
//! A caller presents an RFC 9068 access token from an issuer on the trust
//! list, verified against that issuer's key set or by its RFC 7662
//! introspection endpoint, or, in the explicit edge mode, a proxy in front of
//! the gateway authenticates the caller and signs an assertion the gateway
//! verifies the same way. There is no unauthenticated mode: with no issuer,
//! every guarded request is refused. The configuration keys are our own
//! design; no specification governs them.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::Secret;
use http::HeaderName;
use jsonwebtoken::jwk::JwkSet;
use serde::Deserialize;
use url::Url;

use crate::config::error::Error;
use crate::config::secrets::secret;
use crate::config::transport;

/// The most clock skew `auth.clock_skew_s` may allow, in seconds.
///
/// RFC 7519 §4.1.4 lets a verifier allow "some small leeway, usually no more
/// than a few minutes"; five minutes is the bound.
pub const MAX_CLOCK_SKEW_S: u64 = 300;

/// `[auth]` as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// `token`, the default: every caller presents a bearer access token.
    /// `edge`: a proxy authenticates every caller and signs an assertion the
    /// gateway verifies, in the header `[auth.edge]` names.
    pub mode: Mode,
    /// The audience every token must name in `aud`: this gateway's own
    /// identifier at its issuers (RFC 9068 §4).
    pub audience: Option<String>,
    /// The clock skew allowed on `exp` and `nbf`, at most
    /// [`MAX_CLOCK_SKEW_S`].
    pub clock_skew_s: u64,
    /// How long a fetched or read key set is used before it is fetched
    /// again.
    pub key_set_max_age_s: u64,
    /// The least time between two fetches of one key set: a token naming a
    /// key the cached set does not hold fetches the set again at most this
    /// often.
    pub key_set_refetch_s: u64,
    /// How long a key set fetch or an introspection call may take.
    pub fetch_timeout_ms: u64,
    /// The purpose-of-use rule (§13.4).
    pub purpose_of_use: PurposeOfUse,
    /// The issuers on the trust list, one `[[auth.issuer]]` each.
    pub issuer: Vec<TrustedIssuer>,
    /// The edge mode's header (`[auth.edge]`), set only with
    /// `mode = "edge"`.
    pub edge: Option<Edge>,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            mode: Mode::Token,
            audience: None,
            clock_skew_s: 60,
            key_set_max_age_s: 600,
            key_set_refetch_s: 30,
            fetch_timeout_ms: 5_000,
            purpose_of_use: PurposeOfUse::default(),
            issuer: Vec::new(),
            edge: None,
        }
    }
}

/// How callers reach the gateway.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Mode {
    /// Every caller presents an RFC 6750 bearer access token.
    #[default]
    Token,
    /// A proxy in front of the gateway authenticates every caller and signs
    /// an assertion of who it authenticated.
    Edge,
}

/// `[auth.purpose_of_use]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PurposeOfUse {
    /// Whether a token must carry a purpose of use; `true` by default, and a
    /// deployment that sets `false` records why in its §13.4 answers.
    pub required: bool,
}

impl Default for PurposeOfUse {
    fn default() -> Self {
        Self { required: true }
    }
}

/// One `[[auth.issuer]]`: an authorization server on the trust list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedIssuer {
    /// The issuer identifier its tokens carry in `iss`.
    pub issuer: String,
    /// The URL of its JWK Set.
    pub jwks_uri: Option<String>,
    /// A file holding its JWK Set, for keys handed over out of band.
    pub jwks_file: Option<PathBuf>,
    /// Its JWK Set itself, as JSON text, for keys handed over with the
    /// configuration.
    pub jwks: Option<String>,
    /// Its RFC 7662 introspection endpoint, in place of a key set.
    pub introspection_endpoint: Option<String>,
    /// The client id the gateway authenticates to the introspection endpoint
    /// with.
    pub client_id: Option<String>,
    /// The client secret the gateway authenticates to the introspection
    /// endpoint with.
    pub client_secret: Option<Secret>,
    /// A file holding that client secret, read at boot.
    pub client_secret_file: Option<PathBuf>,
    /// The `client_id`s of the backend clients whose `system/aql-*` grant is
    /// honoured.
    pub backend_clients: Vec<String>,
    /// The `client_id`s of the clients admitted to the DEMOGRAPHIC API,
    /// which no SMART on openEHR resource scope covers; none by default.
    pub demographic_clients: Vec<String>,
    /// The scope value that admits a caller of this issuer to the read-only
    /// operator surface, `{base}/operator/`; absent by default, and then no
    /// caller of this issuer reaches it.
    pub operator_scope: Option<String>,
    /// The opt-in that honours this issuer's `patient/` grants
    /// (`[auth.issuer.patient]`); absent by default, and then a `patient/`
    /// grant of this issuer admits nothing.
    pub patient: Option<PatientIssuer>,
    /// The claims of this issuer's tokens that name who asks for the data
    /// (`[auth.issuer.requester]`), which the consent pre-filter asks Mitz
    /// about; absent by default, and then no caller of this issuer names a
    /// requester.
    pub requester: Option<RequesterClaims>,
}

/// `[auth.issuer.requester]`: the names of the token claims that carry the
/// professional's UZI number and role and the organisation's URA and type,
/// each a string claim.
///
/// No specification the gateway binds names these claims, so each is
/// configured, with no default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RequesterClaims {
    /// The claim carrying the professional's UZI number.
    pub professional: String,
    /// The claim carrying the professional's UZI role code.
    pub role: String,
    /// The claim carrying the organisation's URA.
    pub organisation: String,
    /// The claim carrying the organisation's care provider type.
    pub organisation_type: String,
}

/// `[auth.issuer.patient]`: the one member endpoint whose platform issues
/// this issuer's patient tokens, and the identifier system of that member's
/// `ehr_id`s at the cross-reference.
///
/// A token's `ehrId` claim (SMART on openEHR, master04 §Capabilities,
/// master07 §Context Selection) is read as an identifier in that system and
/// resolved through the cross-reference to each member's `ehr_id` (§5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PatientIssuer {
    /// The registry endpoint id of the member whose platform issues the
    /// tokens.
    pub endpoint: String,
    /// The identifier system whose values are that member's `ehr_id`s, as
    /// the cross-reference names it.
    pub ehr_id_system: String,
}

/// `[auth.edge]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Edge {
    /// The request header the edge's signed assertion travels in.
    pub header: String,
}

/// `[auth]`, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSettings {
    /// Where the credential travels.
    pub mode: AuthMode,
    /// The audience every token must name; `None` only with no issuer.
    pub audience: Option<String>,
    /// The clock skew allowed on `exp` and `nbf`.
    pub clock_skew: Duration,
    /// How long a key set is used before it is fetched again.
    pub key_set_max_age: Duration,
    /// The least time between two fetches of one key set.
    pub key_set_refetch: Duration,
    /// How long a key set fetch or an introspection call may take.
    pub fetch_timeout: Duration,
    /// Whether a token must carry a purpose of use (§13.4).
    pub purpose_required: bool,
    /// The issuers on the trust list, in the order written.
    pub issuers: Vec<IssuerSettings>,
}

impl Default for AuthSettings {
    /// The settings of an absent `[auth]`: no issuer, so every guarded
    /// request is refused.
    fn default() -> Self {
        let written = Auth::default();
        Self {
            mode: AuthMode::Token,
            audience: None,
            clock_skew: Duration::from_secs(written.clock_skew_s),
            key_set_max_age: Duration::from_secs(written.key_set_max_age_s),
            key_set_refetch: Duration::from_secs(written.key_set_refetch_s),
            fetch_timeout: Duration::from_millis(written.fetch_timeout_ms),
            purpose_required: written.purpose_of_use.required,
            issuers: Vec::new(),
        }
    }
}

/// Where a caller's credential travels, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthMode {
    /// An RFC 6750 bearer token in `Authorization`.
    Token,
    /// The edge's signed assertion, in this header.
    Edge(HeaderName),
}

/// One issuer on the trust list, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerSettings {
    /// The issuer identifier its tokens carry in `iss`.
    pub issuer: String,
    /// How its tokens are verified.
    pub verification: Verification,
    /// The backend clients whose `system/aql-*` grant is honoured.
    pub backend_clients: BTreeSet<String>,
    /// The clients admitted to the DEMOGRAPHIC API.
    pub demographic_clients: BTreeSet<String>,
    /// The scope value that admits a caller to the operator surface, when
    /// this issuer may admit one.
    pub operator_scope: Option<String>,
    /// Where this issuer's `patient/` grants are confined, when they are
    /// honoured at all.
    pub patient: Option<PatientBinding>,
    /// The claims that name who asks for the data, when this issuer's tokens
    /// carry them.
    pub requester: Option<RequesterClaims>,
}

/// An issuer's patient tokens bound to one member, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientBinding {
    /// The endpoint of the member whose platform issues the tokens.
    pub endpoint: EndpointId,
    /// The identifier system of that member's `ehr_id`s at the
    /// cross-reference.
    pub ehr_id_system: IdentifierNamespace,
}

/// How an issuer's tokens are verified.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verification {
    /// As signed JWTs, against this key set.
    KeySet(KeySource),
    /// By RFC 7662 introspection at the issuer.
    Introspection(Introspection),
}

/// Where an issuer's JWK Set is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeySource {
    /// Fetched from this URL.
    Uri(Url),
    /// Read from this file.
    File(PathBuf),
    /// Handed over with the configuration.
    Set(JwkSet),
}

/// An issuer's RFC 7662 introspection endpoint and the gateway's client
/// credentials there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Introspection {
    /// The endpoint.
    pub endpoint: Url,
    /// The gateway's client id.
    pub client_id: String,
    /// The gateway's client secret.
    pub client_secret: Secret,
}

/// Why `[auth]` is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthFault {
    /// The clock skew passes [`MAX_CLOCK_SKEW_S`].
    SkewTooLarge,
    /// Two issuers carry the same identifier.
    DuplicateIssuer,
    /// An issuer names no way, or more than one way, to verify its tokens.
    Verification,
    /// A client id or secret is set beside a key set, which needs neither.
    ClientWithoutIntrospection,
    /// More than one issuer is introspected, so an opaque token, which names
    /// no issuer, could go to either.
    SeveralIntrospection,
    /// The inline key set is not a JWK Set.
    KeySet,
    /// The header is no HTTP field name.
    HeaderName,
    /// The edge mode needs exactly one issuer, verified by its key set.
    EdgeIssuer,
    /// `[auth.edge]` is set without `mode = "edge"`.
    EdgeWithoutMode,
    /// The operator scope is not one scope token.
    OperatorScope,
}

impl fmt::Display for AuthFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SkewTooLarge => "allows more than 300 seconds of clock skew",
            Self::DuplicateIssuer => "names an issuer another [[auth.issuer]] already names",
            Self::Verification => {
                "names no way, or more than one way, to verify its tokens: set one of jwks_uri, jwks_file, jwks and introspection_endpoint"
            }
            Self::ClientWithoutIntrospection => {
                "sets client_id or client_secret, which only introspection_endpoint uses"
            }
            Self::SeveralIntrospection => {
                "introspects at a second issuer: an opaque token names no issuer, so one issuer at most is introspected"
            }
            Self::KeySet => "is not a JWK Set (RFC 7517 §5)",
            Self::HeaderName => "is not an HTTP field name",
            Self::EdgeIssuer => {
                "needs exactly one [[auth.issuer]], the edge, verified by jwks_uri, jwks_file or jwks"
            }
            Self::EdgeWithoutMode => "is set, but mode is not \"edge\"",
            Self::OperatorScope => "is not one scope token (RFC 6749 §3.3)",
        })
    }
}

impl Auth {
    /// Resolves `[auth]`, reading every `_file` secret.
    ///
    /// # Errors
    /// Returns [`Error::Auth`] naming the key and the [`AuthFault`],
    /// [`Error::Missing`] for an audience, an issuer identifier, an edge
    /// header, an introspection client credential or a patient binding's
    /// endpoint or system that is not set, [`Error::PatientEndpoint`] for a
    /// patient binding's endpoint that is no endpoint id,
    /// [`Error::Zero`] for a zero duration, [`Error::Url`] and
    /// [`Error::UrlCredentials`] for a URL that does not parse or carries
    /// credentials, and the secret errors of a `_file`.
    pub fn resolve(&self) -> Result<AuthSettings, Error> {
        if self.clock_skew_s > MAX_CLOCK_SKEW_S {
            return Err(fault("auth.clock_skew_s", AuthFault::SkewTooLarge));
        }
        let key_set_max_age = positive_s("auth.key_set_max_age_s", self.key_set_max_age_s)?;
        let key_set_refetch = positive_s("auth.key_set_refetch_s", self.key_set_refetch_s)?;
        if self.fetch_timeout_ms == 0 {
            return Err(Error::Zero {
                key: String::from("auth.fetch_timeout_ms"),
            });
        }
        let mut issuers: Vec<IssuerSettings> = Vec::with_capacity(self.issuer.len());
        for (index, written) in self.issuer.iter().enumerate() {
            let resolved = resolve_issuer(&format!("auth.issuer[{index}]"), written)?;
            if issuers.iter().any(|held| held.issuer == resolved.issuer) {
                return Err(fault(
                    &format!("auth.issuer[{index}].issuer"),
                    AuthFault::DuplicateIssuer,
                ));
            }
            let introspected = |issuer: &IssuerSettings| {
                matches!(issuer.verification, Verification::Introspection(_))
            };
            if introspected(&resolved) && issuers.iter().any(introspected) {
                return Err(fault(
                    &format!("auth.issuer[{index}].introspection_endpoint"),
                    AuthFault::SeveralIntrospection,
                ));
            }
            issuers.push(resolved);
        }
        let audience = self
            .audience
            .clone()
            .filter(|audience| !audience.is_empty());
        if audience.is_none() && !issuers.is_empty() {
            return Err(Error::Missing {
                key: String::from("auth.audience"),
            });
        }
        let mode = self.resolve_mode(&issuers)?;
        Ok(AuthSettings {
            mode,
            audience,
            clock_skew: Duration::from_secs(self.clock_skew_s),
            key_set_max_age,
            key_set_refetch,
            fetch_timeout: Duration::from_millis(self.fetch_timeout_ms),
            purpose_required: self.purpose_of_use.required,
            issuers,
        })
    }

    /// Resolves the mode: the edge mode needs its header and exactly one
    /// issuer verified by its key set.
    fn resolve_mode(&self, issuers: &[IssuerSettings]) -> Result<AuthMode, Error> {
        match (self.mode, &self.edge) {
            (Mode::Token, None) => Ok(AuthMode::Token),
            (Mode::Token, Some(_)) => Err(fault("auth.edge", AuthFault::EdgeWithoutMode)),
            (Mode::Edge, None) => Err(Error::Missing {
                key: String::from("auth.edge.header"),
            }),
            (Mode::Edge, Some(edge)) => {
                if edge.header.is_empty() {
                    return Err(Error::Missing {
                        key: String::from("auth.edge.header"),
                    });
                }
                let header = HeaderName::from_bytes(edge.header.as_bytes())
                    .map_err(|_name| fault("auth.edge.header", AuthFault::HeaderName))?;
                match issuers {
                    [issuer] if matches!(issuer.verification, Verification::KeySet(_)) => {
                        Ok(AuthMode::Edge(header))
                    }
                    _ => Err(fault("auth.issuer", AuthFault::EdgeIssuer)),
                }
            }
        }
    }
}

/// Resolves one `[[auth.issuer]]` at `key`.
fn resolve_issuer(key: &str, written: &TrustedIssuer) -> Result<IssuerSettings, Error> {
    if written.issuer.is_empty() {
        return Err(Error::Missing {
            key: format!("{key}.issuer"),
        });
    }
    let client_secret = secret(
        &format!("{key}.client_secret"),
        written.client_secret.as_ref(),
        written.client_secret_file.as_deref(),
    )?;
    let verification = match (
        &written.jwks_uri,
        &written.jwks_file,
        &written.jwks,
        &written.introspection_endpoint,
    ) {
        (None, None, Some(text), None) => Verification::KeySet(KeySource::Set(
            serde_json::from_str(text)
                .map_err(|_shape| fault(&format!("{key}.jwks"), AuthFault::KeySet))?,
        )),
        (Some(uri), None, None, None) => {
            Verification::KeySet(KeySource::Uri(url(&format!("{key}.jwks_uri"), uri)?))
        }
        (None, Some(file), None, None) => Verification::KeySet(KeySource::File(file.clone())),
        (None, None, None, Some(endpoint)) => {
            let endpoint = url(&format!("{key}.introspection_endpoint"), endpoint)?;
            let client_id = written
                .client_id
                .clone()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Error::Missing {
                    key: format!("{key}.client_id"),
                })?;
            let client_secret = client_secret.ok_or_else(|| Error::Missing {
                key: format!("{key}.client_secret"),
            })?;
            Verification::Introspection(Introspection {
                endpoint,
                client_id,
                client_secret,
            })
        }
        _ => return Err(fault(key, AuthFault::Verification)),
    };
    if matches!(verification, Verification::KeySet(_))
        && (written.client_id.is_some()
            || written.client_secret.is_some()
            || written.client_secret_file.is_some())
    {
        return Err(fault(
            &format!("{key}.client_id"),
            AuthFault::ClientWithoutIntrospection,
        ));
    }
    let patient = written
        .patient
        .as_ref()
        .map(|patient| resolve_patient(&format!("{key}.patient"), patient))
        .transpose()?;
    if let Some(requester) = &written.requester {
        for (name, claim) in [
            ("professional", &requester.professional),
            ("role", &requester.role),
            ("organisation", &requester.organisation),
            ("organisation_type", &requester.organisation_type),
        ] {
            if claim.is_empty() {
                return Err(Error::Missing {
                    key: format!("{key}.requester.{name}"),
                });
            }
        }
    }
    // NOTE: RFC 6749 §3.3: a scope token is one or more %x21 / %x23-5B / %x5D-7E,
    // so the operator scope is matched as one whole token of the `scope` claim.
    if let Some(scope) = &written.operator_scope
        && (scope.is_empty()
            || !scope
                .bytes()
                .all(|byte| matches!(byte, 0x21 | 0x23..=0x5B | 0x5D..=0x7E)))
    {
        return Err(fault(
            &format!("{key}.operator_scope"),
            AuthFault::OperatorScope,
        ));
    }
    Ok(IssuerSettings {
        issuer: written.issuer.clone(),
        verification,
        backend_clients: written.backend_clients.iter().cloned().collect(),
        demographic_clients: written.demographic_clients.iter().cloned().collect(),
        operator_scope: written.operator_scope.clone(),
        patient,
        requester: written.requester.clone(),
    })
}

/// Resolves one `[auth.issuer.patient]` at `key`: an endpoint id and a
/// system, both set.
///
/// Whether the registry holds the endpoint is checked where the registry is
/// read, as for every other endpoint the configuration names.
fn resolve_patient(key: &str, written: &PatientIssuer) -> Result<PatientBinding, Error> {
    if written.endpoint.is_empty() {
        return Err(Error::Missing {
            key: format!("{key}.endpoint"),
        });
    }
    let endpoint =
        EndpointId::new(written.endpoint.as_str()).map_err(|source| Error::PatientEndpoint {
            key: format!("{key}.endpoint"),
            source,
        })?;
    let ehr_id_system =
        IdentifierNamespace::new(written.ehr_id_system.as_str()).map_err(|_empty| {
            Error::Missing {
                key: format!("{key}.ehr_id_system"),
            }
        })?;
    Ok(PatientBinding {
        endpoint,
        ehr_id_system,
    })
}

/// Parses the URL at `key`, held to the trust-anchor policy of
/// [`transport::trust_anchor`], with no credentials in it.
fn url(key: &str, text: &str) -> Result<Url, Error> {
    let url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::UrlCredentials {
            key: key.to_owned(),
            section: key
                .rsplit_once('.')
                .map_or(key, |(section, _)| section)
                .to_owned(),
        });
    }
    // NOTE: RFC 7662 §4, the introspection endpoint is protected by TLS; a key
    // set is held to the same trust-anchor policy, loopback http being our own design.
    transport::trust_anchor(key, &url)?;
    Ok(url)
}

/// The refusal of `key` for `fault`.
fn fault(key: &str, fault: AuthFault) -> Error {
    Error::Auth {
        key: key.to_owned(),
        fault,
    }
}

/// The duration of `seconds` at `key`, refusing zero.
fn positive_s(key: &str, seconds: u64) -> Result<Duration, Error> {
    if seconds == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(Duration::from_secs(seconds))
}
