// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[trust]`: FerroFED's policy over the callers `[auth]` verifies, as
//! written and as resolved.
//!
//! One `[[trust.issuer]]` entry per issuer says what its tokens are honoured
//! for beyond their `user/` and `system/` grants: the backend clients whose
//! `system/aql-*` grant counts, the clients admitted to the DEMOGRAPHIC API,
//! the member its `patient/` grants are bound to, the claims of the
//! requester, the assurance and the professional, and its declaration as a
//! national contact point. An entry with an `introspection_endpoint` adds
//! the one issuer whose tokens are verified by RFC 7662 introspection in
//! place of a key set. `mode = "edge"` with `[trust.edge]` reads the
//! credential from the header a trusted proxy signs its assertion in, and
//! `[trust.purpose_of_use]` sets the §13.4 rule. No specification governs
//! the keys: our own design.

use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use http::HeaderName;
use serde::Deserialize;
use url::Url;

use crate::config::auth::assurance::AssuranceClaims;
use crate::config::auth::contact_point::ContactPointClaims;
use crate::config::auth::requester::RequesterClaims;
use crate::config::auth::{
    AuthFault, AuthMode, Introspection, IssuerSettings, PatientBinding, Verification, fault,
};
use crate::config::error::Error;
use crate::config::transport;
use ferrohealth_secret::file::read_secret;

/// `[trust]` as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Trust {
    /// `token`, the default: every caller presents a bearer access token.
    /// `edge`: a proxy authenticates every caller and signs an assertion the
    /// gateway verifies, in the header `[trust.edge]` names.
    pub mode: Mode,
    /// The purpose-of-use rule (§13.4).
    pub purpose_of_use: PurposeOfUse,
    /// The policy of each trusted issuer, one `[[trust.issuer]]` each.
    pub issuer: Vec<TrustedIssuer>,
    /// The edge mode's header (`[trust.edge]`), set only with
    /// `mode = "edge"`.
    pub edge: Option<Edge>,
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

/// `[trust.purpose_of_use]`.
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

/// `[trust.edge]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Edge {
    /// The request header the edge's signed assertion travels in.
    pub header: String,
}

/// One `[[trust.issuer]]`: the policy over one trusted issuer's tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrustedIssuer {
    /// The issuer identifier its tokens carry in `iss`: an `[auth]` issuer,
    /// or the introspected one.
    pub issuer: String,
    /// Its RFC 7662 introspection endpoint, for an issuer `[auth]` does not
    /// verify by its key set.
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
    /// The opt-in that honours this issuer's `patient/` grants
    /// (`[trust.issuer.patient]`); absent by default, and then a `patient/`
    /// grant of this issuer admits nothing.
    pub patient: Option<PatientIssuer>,
    /// The claims of this issuer's tokens that name who asks for the data
    /// (`[trust.issuer.requester]`), which the consent pre-filter asks Mitz
    /// about; absent by default, and then no caller of this issuer names a
    /// requester.
    pub requester: Option<RequesterClaims>,
    /// The claim and the values that carry the authentication assurance of
    /// this issuer's tokens, and the least level a patient-data request
    /// needs (`[trust.issuer.assurance]`); absent by default, and then no
    /// level is read or required.
    pub assurance: Option<AssuranceClaims>,
    /// Whether this issuer's client tokens, which name no natural person,
    /// act for the professional they name; `false` by default, and then such
    /// a token reaches no patient data.
    pub client_tokens_act_for_professional: bool,
    /// The name of the string claim of this issuer's tokens that carries
    /// the name of the agency that issued the professional's
    /// `national_provider_identifier`, the `issuing_authority_name` of
    /// Implementing Regulation (EU) 2026/2099 Annex Table 1; absent by
    /// default, and then none is read or conveyed.
    pub professional_issuing_authority: Option<String>,
    /// The declaration of this issuer as a national contact point for
    /// digital health, with the claims of its tokens that carry the
    /// Implementing Regulation (EU) 2026/2099 Annex attributes
    /// (`[trust.issuer.national_contact_point]`); absent by default.
    pub national_contact_point: Option<ContactPointClaims>,
}

/// `[trust.issuer.patient]`: the one member endpoint whose platform issues
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
    /// Whether the RFC 8693 `act` claim of this issuer's patient tokens
    /// names a natural person who acts for the patient, a proxy or a legal
    /// representative (Regulation (EU) 2025/327 Art 4(2)); `false` by
    /// default, and then no caller of this issuer is read as a
    /// representative.
    pub act_is_representative: bool,
}

/// What `[auth]` lends the introspected issuer: the audiences an answer
/// must name, the clock skew, and how long one call may take.
#[derive(Debug, Clone)]
pub(super) struct Timing {
    /// The audiences.
    pub(super) audience: Vec<String>,
    /// The clock skew.
    pub(super) clock_skew: Duration,
    /// How long one call may take.
    pub(super) timeout: Duration,
}

impl Trust {
    /// Resolves `[trust]` over `keyed`, the issuers `[auth]` verifies by
    /// their key sets, returning the mode, whether a purpose of use is
    /// required, and every issuer with its policy: `keyed` in order, then
    /// the introspected one.
    ///
    /// # Errors
    /// Returns [`Error::Auth`] naming the key and the [`AuthFault`],
    /// [`Error::Missing`] for an issuer identifier, an edge header, an
    /// introspection client credential or a patient binding's endpoint or
    /// system that is not set, [`Error::PatientEndpoint`] for a patient
    /// binding's endpoint that is no endpoint id, [`Error::Url`],
    /// [`Error::UrlCredentials`] and [`Error::TrustAnchor`] for an
    /// introspection endpoint that does not parse, carries credentials or
    /// travels in the clear, and the secret errors of a `_file`.
    pub(super) fn resolve(
        &self,
        keyed: Vec<KeySetSettings>,
        timing: &Timing,
    ) -> Result<(AuthMode, bool, Vec<IssuerSettings>), Error> {
        let mut issuers: Vec<IssuerSettings> = keyed
            .into_iter()
            .map(|settings| {
                IssuerSettings::new(
                    settings.issuer.clone(),
                    Verification::KeySet(Box::new(settings)),
                )
            })
            .collect();
        let mut named: Vec<&str> = Vec::new();
        for (index, written) in self.issuer.iter().enumerate() {
            let key = format!("trust.issuer[{index}]");
            if written.issuer.is_empty() {
                return Err(Error::Missing {
                    key: format!("{key}.issuer"),
                });
            }
            if named.contains(&written.issuer.as_str()) {
                return Err(fault(&format!("{key}.issuer"), AuthFault::DuplicateIssuer));
            }
            named.push(&written.issuer);
            let introspection = introspection(&key, written, timing)?;
            let held = issuers
                .iter()
                .position(|issuer| issuer.issuer == written.issuer);
            let at = match (held, introspection) {
                (Some(_), Some(_)) => {
                    return Err(fault(
                        &format!("{key}.introspection_endpoint"),
                        AuthFault::TwoVerifications,
                    ));
                }
                (Some(at), None) => at,
                (None, Some(introspection)) => {
                    let introspected = |issuer: &IssuerSettings| {
                        matches!(issuer.verification, Verification::Introspection(_))
                    };
                    if issuers.iter().any(introspected) {
                        return Err(fault(
                            &format!("{key}.introspection_endpoint"),
                            AuthFault::SeveralIntrospection,
                        ));
                    }
                    issuers.push(IssuerSettings::new(
                        written.issuer.clone(),
                        Verification::Introspection(introspection),
                    ));
                    issuers.len().saturating_sub(1)
                }
                (None, None) => {
                    return Err(fault(&format!("{key}.issuer"), AuthFault::UnknownIssuer));
                }
            };
            let policy = policy(&key, written)?;
            if let Some(issuer) = issuers.get_mut(at) {
                let verification = issuer.verification.clone();
                *issuer = IssuerSettings {
                    issuer: written.issuer.clone(),
                    verification,
                    ..policy
                };
            }
        }
        let mode = self.resolve_mode(&issuers)?;
        Ok((mode, self.purpose_of_use.required, issuers))
    }

    /// Resolves the mode: the edge mode needs its header and exactly one
    /// issuer, verified by its key set.
    fn resolve_mode(&self, issuers: &[IssuerSettings]) -> Result<AuthMode, Error> {
        match (self.mode, &self.edge) {
            (Mode::Token, None) => Ok(AuthMode::Token),
            (Mode::Token, Some(_)) => Err(fault("trust.edge", AuthFault::EdgeWithoutMode)),
            (Mode::Edge, None) => Err(Error::Missing {
                key: String::from("trust.edge.header"),
            }),
            (Mode::Edge, Some(edge)) => {
                if edge.header.is_empty() {
                    return Err(Error::Missing {
                        key: String::from("trust.edge.header"),
                    });
                }
                let header = HeaderName::from_bytes(edge.header.as_bytes())
                    .map_err(|_name| fault("trust.edge.header", AuthFault::HeaderName))?;
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

/// The introspection endpoint the entry at `key` names, with the gateway's
/// client credentials there, or `None` for an entry that names none.
fn introspection(
    key: &str,
    written: &TrustedIssuer,
    timing: &Timing,
) -> Result<Option<Introspection>, Error> {
    let client_secret = read_secret(
        &format!("{key}.client_secret"),
        written.client_secret.clone(),
        written.client_secret_file.as_deref(),
    )?;
    let Some(endpoint) = &written.introspection_endpoint else {
        if written.client_id.is_some() || client_secret.is_some() {
            return Err(fault(
                &format!("{key}.client_id"),
                AuthFault::ClientWithoutIntrospection,
            ));
        }
        return Ok(None);
    };
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
    if timing.audience.iter().all(String::is_empty) {
        return Err(Error::Missing {
            key: String::from("auth.audience"),
        });
    }
    Ok(Some(Introspection {
        endpoint,
        client_id,
        client_secret,
        audience: timing.audience.clone(),
        clock_skew: timing.clock_skew,
        timeout: timing.timeout,
    }))
}

/// The policy the entry at `key` writes, its verification a placeholder the
/// caller replaces.
fn policy(key: &str, written: &TrustedIssuer) -> Result<IssuerSettings, Error> {
    let patient = written
        .patient
        .as_ref()
        .map(|patient| resolve_patient(&format!("{key}.patient"), patient))
        .transpose()?;
    if let Some(requester) = &written.requester {
        requester.check(&format!("{key}.requester"))?;
    }
    let assurance = written
        .assurance
        .as_ref()
        .map(|assurance| assurance.resolve(&format!("{key}.assurance")))
        .transpose()?;
    let professional_issuing_authority = claim_name(
        &format!("{key}.professional_issuing_authority"),
        written.professional_issuing_authority.as_ref(),
    )?;
    let national_contact_point = written
        .national_contact_point
        .as_ref()
        .map(|claims| claims.resolve(&format!("{key}.national_contact_point")))
        .transpose()?;
    Ok(IssuerSettings {
        backend_clients: written.backend_clients.iter().cloned().collect(),
        demographic_clients: written.demographic_clients.iter().cloned().collect(),
        patient,
        requester: written.requester.clone(),
        assurance,
        client_tokens_act_for_professional: written.client_tokens_act_for_professional,
        professional_issuing_authority,
        national_contact_point,
        ..IssuerSettings::new(
            written.issuer.clone(),
            Verification::KeySet(Box::new(KeySetSettings::new(
                written.issuer.clone(),
                String::new(),
            ))),
        )
    })
}

/// Resolves an optional claim name at `key`: absent, or set and not empty.
///
/// # Errors
///
/// Returns [`Error::Missing`] for a claim name written empty.
fn claim_name(key: &str, written: Option<&String>) -> Result<Option<String>, Error> {
    match written {
        Some(name) if name.is_empty() => Err(Error::Missing {
            key: key.to_owned(),
        }),
        written => Ok(written.cloned()),
    }
}

/// Resolves one `[trust.issuer.patient]` at `key`: an endpoint id and a
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
        act_is_representative: written.act_is_representative,
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
    // NOTE: RFC 7662 §4, the introspection endpoint is protected by TLS; loopback
    // http is our own design, as for every other trust anchor.
    transport::trust_anchor(key, &url)?;
    Ok(url)
}
