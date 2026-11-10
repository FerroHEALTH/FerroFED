// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[trust]`: FerroFED's policy over the callers `[auth]` verifies, as
//! written and as resolved.
//!
//! One `[[trust.issuer]]` entry per issuer says what its tokens are honoured
//! for beyond their `user/` and `system/` grants: the backend clients whose
//! `system/aql-*` grant counts, the clients admitted to the DEMOGRAPHIC API,
//! the member its `patient/` grants are bound to and the professional. Each entry names an `[[auth.issuer]]`, which says
//! how its tokens are verified. `mode = "edge"` with `[trust.edge]` reads the
//! credential from the header a trusted proxy signs its assertion in, and
//! `[trust.purpose_of_use]` sets the §13.4 rule. No specification governs
//! the keys: our own design.

use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use ferrohealth_smart::resource::introspection::IntrospectionSettings;
use http::HeaderName;
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::config::auth::{
    AuthFault, AuthMode, IssuerSettings, PatientBinding, Verification, fault,
};
use crate::config::error::Error;

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
    /// The issuer identifier its tokens carry in `iss`, which an
    /// `[[auth.issuer]]` table trusts.
    pub issuer: String,
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
    /// The removed `[trust.issuer.requester]`, refused by name
    /// ([`AuthFault::RequesterMoved`]).
    pub requester: Option<Removed>,
    /// The removed `[trust.issuer.assurance]`, refused by name
    /// ([`AuthFault::AssuranceMoved`]).
    pub assurance: Option<Removed>,
    /// The removed `client_tokens_act_for_professional`, refused by name
    /// ([`AuthFault::ActsForRemoved`]).
    pub client_tokens_act_for_professional: Option<Removed>,
    /// The name of the string claim of this issuer's tokens that carries
    /// the name of the agency that issued the professional's
    /// `national_provider_identifier`, the `issuing_authority_name` of
    /// Implementing Regulation (EU) 2026/2099 Annex Table 1; absent by
    /// default, and then none is read or conveyed.
    pub professional_issuing_authority: Option<String>,
    /// The removed `[trust.issuer.national_contact_point]`, read only so that
    /// a configuration still setting it is refused by name
    /// ([`AuthFault::ContactPointMoved`]); its value is never kept.
    pub national_contact_point: Option<Removed>,
}

/// A table removed in 0.0.17, read only so that a configuration still setting it is
/// refused by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Removed;

impl<'de> Deserialize<'de> for Removed {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        IgnoredAny::deserialize(deserializer).map(|_ignored| Self)
    }
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
    /// The retired `act_is_representative`, which the issuer's
    /// `act_represents_patient` replaced in 0.0.19: set, it refuses the
    /// configuration.
    #[serde(skip_serializing)]
    pub act_is_representative: ferrohealth_config::removed::Removed,
}

impl Trust {
    /// Resolves `[trust]` over `keyed`, the issuers `[auth]` verifies by
    /// their key sets, and `introspected`, the ones it verifies by RFC 7662
    /// introspection, returning the mode, whether a purpose of use is
    /// required, and every issuer with its policy: `keyed` in order, then the
    /// introspected one.
    ///
    /// # Errors
    /// Returns [`Error::Auth`] naming the key and the [`AuthFault`],
    /// [`Error::Missing`] for an issuer identifier, an edge header or a
    /// patient binding's endpoint or system that is not set, and
    /// [`Error::PatientEndpoint`] for a patient binding's endpoint that is no
    /// endpoint id.
    pub(super) fn resolve(
        &self,
        keyed: Vec<KeySetSettings>,
        introspected: Vec<IntrospectionSettings>,
    ) -> Result<(AuthMode, bool, Vec<IssuerSettings>), Error> {
        // NOTE: no specification governs this: our own design; an opaque token names
        // no issuer, so one issuer at most is introspected.
        if introspected.len() > 1 {
            return Err(fault("auth.issuer", AuthFault::SeveralIntrospection));
        }
        let mut issuers: Vec<IssuerSettings> = keyed
            .into_iter()
            .map(|settings| {
                IssuerSettings::new(
                    settings.issuer.clone(),
                    Verification::KeySet(Box::new(settings)),
                )
            })
            .chain(introspected.into_iter().map(|settings| {
                IssuerSettings::new(
                    settings.issuer.clone(),
                    Verification::Introspection(Box::new(settings)),
                )
            }))
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
            let issuer = issuers
                .iter_mut()
                .find(|issuer| issuer.issuer == written.issuer)
                .ok_or_else(|| fault(&format!("{key}.issuer"), AuthFault::UnknownIssuer))?;
            *issuer = policy(&key, written, issuer.verification.clone())?;
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

/// The issuer the entry at `key` writes the policy of, its tokens verified
/// as `verification` says.
fn policy(
    key: &str,
    written: &TrustedIssuer,
    verification: Verification,
) -> Result<IssuerSettings, Error> {
    let patient = written
        .patient
        .as_ref()
        .map(|patient| resolve_patient(&format!("{key}.patient"), patient))
        .transpose()?;
    for (removed, name, refusal) in [
        (written.requester, "requester", AuthFault::RequesterMoved),
        (written.assurance, "assurance", AuthFault::AssuranceMoved),
        (
            written.client_tokens_act_for_professional,
            "client_tokens_act_for_professional",
            AuthFault::ActsForRemoved,
        ),
    ] {
        if removed.is_some() {
            return Err(fault(&format!("{key}.{name}"), refusal));
        }
    }
    let professional_issuing_authority = claim_name(
        &format!("{key}.professional_issuing_authority"),
        written.professional_issuing_authority.as_ref(),
    )?;
    if written.national_contact_point.is_some() {
        return Err(fault(
            &format!("{key}.national_contact_point"),
            AuthFault::ContactPointMoved,
        ));
    }
    Ok(IssuerSettings {
        backend_clients: written.backend_clients.iter().cloned().collect(),
        demographic_clients: written.demographic_clients.iter().cloned().collect(),
        patient,
        professional_issuing_authority,
        ..IssuerSettings::new(written.issuer.clone(), verification)
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
    })
}
