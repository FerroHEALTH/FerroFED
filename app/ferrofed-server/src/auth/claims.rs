// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The claims the gateway reads from a verified access token or an RFC 7662
//! introspection answer, typed.
//!
//! An access token carries the claims RFC 9068 §2.2 requires (`iss`, `exp`,
//! `aud`, `sub`, `client_id`, `iat`, `jti`) and may carry `scope`, the IHE
//! IUA extension (ITI TF-2 3.71.4.2.2.1.1: `extensions.ihe_iua`, its
//! `purpose_of_use` an array of FHIR `Coding`) and RFC 9396
//! `authorization_details`, whose `purpose_of_use` is written `system|code`
//! (the Federation Tier's Annex B §B.4a.3), and the SMART on openEHR `ehrId`,
//! the `ehr_id` of the launch context, "conveyed via the `ehrId` token claim"
//! (master04 §Capabilities). The string claims an issuer's
//! `[trust.issuer.requester]` names are read as the requester the consent
//! pre-filter asks about (§13.4). The IUA `subject_name`,
//! `national_provider_identifier` and `subject_role` are read as the
//! professional's identification, the string claim an issuer's
//! `professional_issuing_authority` names as the agency that issued the
//! identifier, and the string claim an issuer's
//! `[trust.issuer.assurance]` names as the authentication assurance, by
//! default `acr` (RFC 9068 §2.2.1). For an issuer declared a national contact
//! point, the IUA `subject_organization` and `subject_role` and the string
//! claims its `[trust.issuer.national_contact_point]` names are read as the
//! Implementing Regulation (EU) 2026/2099 Annex attributes. The `iss` and
//! `sub` of the RFC 8693 §4.1 `act` claim are read as the current actor,
//! which a patient token of an issuer that declares `act_is_representative`
//! names as the patient's representative. Every other claim is passed over,
//! and the
//! IUA `person_id`, a patient identifier, is never read (§5.4.1, N33).
//!
//! IUA is cited from the Revision 2.5 Trial Implementation supplement
//! vendored at `docs/specs/ihe-iua/IHE_ITI_Suppl_IUA.md`: ITI TF-2 3.71.4.2.2.1
//! (the JSON Web Token Option) and 3.71.4.2.2.1.1 (the JWT IUA extension).

use std::collections::BTreeMap;

use ferrofed_engine::conveyance::relayed::Role;
use ferrofed_identity::role::consent::Requester;
use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::auth::caller::{Actor, Professional, PurposeOfUse};
use crate::config::auth::IssuerSettings;
use crate::config::auth::requester::RequesterClaims;

/// The claims of a verified caller the gateway reads beside its `sub`,
/// `client_id`, `scope` and `ehrId`, which `ferrohealth-smart` reads: the
/// claims of a verified access token, or the members of an introspection
/// answer.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Claims {
    /// The organisation and the purposes of use.
    #[serde(flatten)]
    declared: Declared,
    /// Every other claim.
    #[serde(flatten)]
    others: Others,
}

impl Claims {
    /// Reads `claims`, every claim of a verified caller.
    ///
    /// # Errors
    ///
    /// Returns the JSON reader's error for a claim the gateway reads that
    /// does not have its shape.
    #[expect(
        clippy::disallowed_types,
        reason = "the token-claims seam: ferrohealth-smart hands a verified caller's claims over as a JSON map"
    )]
    pub(super) fn read(
        claims: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, serde_json::Error> {
        serde_json::from_value(serde_json::Value::Object(claims.clone()))
    }

    /// Returns the string claim `name`, the claim an issuer's
    /// `[trust.issuer.assurance]` names, when the token carries it.
    pub(super) fn text(&self, name: &str) -> Option<String> {
        self.others.text(name)
    }

    /// Returns the professional's identification the token states, the
    /// identifier's issuing authority read from the claim `authority` names.
    pub(super) fn professional(&self, authority: Option<&str>) -> Professional {
        self.declared
            .professional(authority.and_then(|claim| self.others.text(claim)))
    }

    /// Returns the requester the claims `named` name, when the token carries
    /// every one of them as a string.
    pub(super) fn requester(&self, named: Option<&RequesterClaims>) -> Option<Requester> {
        named.and_then(|named| self.others.requester(named))
    }

    /// Returns the representative of the patient the token names, as
    /// [`Declared::representative`] reads it.
    pub(super) fn representative(&self, issuer: &IssuerSettings) -> Option<Actor> {
        self.declared.representative(issuer)
    }

    /// Returns what the IUA extension states that the 2026/2099 Annex also
    /// asks for.
    pub(super) fn annex(&self) -> IuaAnnex {
        self.declared.annex()
    }

    /// Returns the organisation the token names.
    pub(super) fn organisation(&self) -> Option<String> {
        self.declared.organisation()
    }

    /// Returns every purpose of use the token declares.
    pub(super) fn purposes(&self) -> Vec<PurposeOfUse> {
        self.declared.purposes()
    }
}

/// An RFC 7662 §2.2 introspection answer: the members the gateway checks.
#[derive(Debug, Deserialize)]
pub(super) struct Introspected {
    /// `active`, the one member RFC 7662 §2.2 requires.
    pub(super) active: bool,
    /// `iss`, which must name the issuer asked when it is present.
    #[serde(default)]
    pub(super) iss: Option<String>,
    /// `sub`.
    #[serde(default)]
    pub(super) sub: Option<String>,
    /// `client_id`.
    #[serde(default)]
    pub(super) client_id: Option<String>,
    /// `scope`.
    #[serde(default)]
    pub(super) scope: Option<String>,
    /// `aud`.
    #[serde(default)]
    pub(super) aud: Option<Audience>,
    /// `exp`, in seconds since the epoch.
    #[serde(default)]
    pub(super) exp: Option<i64>,
    /// `nbf`, in seconds since the epoch.
    #[serde(default)]
    pub(super) nbf: Option<i64>,
}

/// The claims a token carries besides the ones read by name: a string claim
/// kept as its text, any other skipped.
///
/// `Debug` shows how many there are and none of them.
#[derive(Default, Deserialize)]
pub(super) struct Others(BTreeMap<String, Claim>);

/// One claim of [`Others`].
#[derive(Deserialize)]
#[serde(untagged)]
enum Claim {
    /// A string claim.
    Text(String),
    /// Any other claim, skipped.
    Other(IgnoredAny),
}

impl Others {
    /// The string claim `name`, when the token carries one that is not
    /// empty.
    pub(super) fn text(&self, name: &str) -> Option<String> {
        match self.0.get(name)? {
            Claim::Text(text) if !text.is_empty() => Some(text.clone()),
            Claim::Text(_) | Claim::Other(_) => None,
        }
    }

    /// The requester the claims `named` name, when every one is a string.
    pub(super) fn requester(&self, named: &RequesterClaims) -> Option<Requester> {
        Requester::new(
            self.text(&named.professional)?,
            self.text(&named.role)?,
            self.text(&named.organisation)?,
            self.text(&named.organisation_type)?,
        )
    }
}

impl std::fmt::Debug for Others {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Others")
            .field("claims", &self.0.len())
            .finish()
    }
}

/// An `aud` claim: one audience or several (RFC 7519 §4.1.3).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum Audience {
    /// One audience.
    One(String),
    /// Several audiences.
    Several(Vec<String>),
}

impl Audience {
    /// Whether `audience` is among these.
    pub(super) fn names(&self, audience: &str) -> bool {
        match self {
            Self::One(one) => one == audience,
            Self::Several(several) => several.iter().any(|one| one == audience),
        }
    }
}

/// The organisation and the purposes of use a token declares.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Declared {
    /// The IHE IUA extension's container.
    #[serde(default)]
    extensions: Option<Extensions>,
    /// RFC 9396 `authorization_details`.
    #[serde(default)]
    authorization_details: Option<Vec<AuthorizationDetail>>,
    /// The RFC 8693 §4.1 `act` claim, the current actor.
    #[serde(default)]
    act: Option<ActClaim>,
}

/// The RFC 8693 §4.1 `act` claim: a JSON object whose members are claims
/// that identify the actor, or a value that is no object and identifies no
/// one.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ActClaim {
    /// An object of claims about the actor.
    Claims(Others),
    /// Any other value.
    Other(IgnoredAny),
}

/// The `extensions` claim of IHE IUA.
#[derive(Debug, Deserialize)]
struct Extensions {
    /// `ihe_iua`.
    #[serde(default)]
    ihe_iua: Option<IheIua>,
}

/// The `ihe_iua` extension: only the members the gateway reads.
#[derive(Debug, Deserialize)]
struct IheIua {
    /// `subject_organization_id`, a URI naming the caller's organisation.
    #[serde(default)]
    subject_organization_id: Option<String>,
    /// `subject_name`, the user's name.
    #[serde(default)]
    subject_name: Option<String>,
    /// `subject_organization`, the name of the user's organisation.
    #[serde(default)]
    subject_organization: Option<String>,
    /// `subject_role`, an array of FHIR `Coding`.
    #[serde(default)]
    subject_role: Option<Vec<Coding>>,
    /// `national_provider_identifier`, the identifier the professional's
    /// national authority issued.
    #[serde(default)]
    national_provider_identifier: Option<String>,
    /// `purpose_of_use`, an array of FHIR `Coding`.
    #[serde(default)]
    purpose_of_use: Option<Vec<Coding>>,
}

/// A FHIR `Coding`: only the members the gateway reads.
#[derive(Debug, Deserialize)]
struct Coding {
    /// `system`.
    #[serde(default)]
    system: Option<String>,
    /// `code`.
    #[serde(default)]
    code: Option<String>,
}

/// One RFC 9396 authorization detail: only the member the gateway reads.
#[derive(Debug, Deserialize)]
struct AuthorizationDetail {
    /// `purpose_of_use`, written `system|code`.
    #[serde(default)]
    purpose_of_use: Option<String>,
}

impl Declared {
    /// The IUA `subject_organization_id`, when it is set and not empty.
    pub(super) fn organisation(&self) -> Option<String> {
        self.iua()
            .and_then(|iua| iua.subject_organization_id.clone())
            .filter(|organisation| !organisation.is_empty())
    }

    /// The professional's identification the IUA extension states, each
    /// member when it is set and not empty and each role that has a code,
    /// with the `issuing_authority` the token states outside IUA.
    pub(super) fn professional(&self, issuing_authority: Option<String>) -> Professional {
        let iua = self.iua();
        let text = |value: Option<&String>| value.filter(|value| !value.is_empty()).cloned();
        Professional {
            identifier: text(iua.and_then(|iua| iua.national_provider_identifier.as_ref())),
            name: text(iua.and_then(|iua| iua.subject_name.as_ref())),
            issuing_authority,
            roles: self.roles(),
        }
    }

    /// Each IUA `subject_role` that has a code.
    fn roles(&self) -> Vec<Role> {
        self.iua()
            .and_then(|iua| iua.subject_role.as_deref())
            .unwrap_or_default()
            .iter()
            .filter_map(|coding| {
                let code = coding.code.clone().filter(|code| !code.is_empty())?;
                Some(Role {
                    system: coding.system.clone().filter(|system| !system.is_empty()),
                    code,
                })
            })
            .collect()
    }

    /// What the IUA extension states that Implementing Regulation (EU)
    /// 2026/2099 Annex Tables 1 and 2 also ask for, each member when it is
    /// set and not empty, and each role that has a code.
    pub(super) fn annex(&self) -> IuaAnnex {
        let iua = self.iua();
        let text = |value: Option<&String>| value.filter(|value| !value.is_empty()).cloned();
        IuaAnnex {
            hp_identifier: text(iua.and_then(|iua| iua.national_provider_identifier.as_ref())),
            roles: self.roles(),
            provider_identifier: text(iua.and_then(|iua| iua.subject_organization_id.as_ref())),
            provider_name: text(iua.and_then(|iua| iua.subject_organization.as_ref())),
        }
    }

    /// Every purpose of use declared, IUA first, sorted and without
    /// duplicates; a coding with no code declares none.
    pub(super) fn purposes(&self) -> Vec<PurposeOfUse> {
        let iua = self
            .iua()
            .and_then(|iua| iua.purpose_of_use.as_deref())
            .unwrap_or_default()
            .iter()
            .filter_map(|coding| {
                let code = coding.code.clone().filter(|code| !code.is_empty())?;
                Some(PurposeOfUse {
                    system: coding.system.clone().filter(|system| !system.is_empty()),
                    code,
                })
            });
        let rar = self
            .authorization_details
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|detail| detail.purpose_of_use.as_deref().and_then(token));
        let mut purposes: Vec<PurposeOfUse> = iua.chain(rar).collect();
        purposes.sort();
        purposes.dedup();
        purposes
    }

    /// The representative of the patient the token names, when `issuer`'s
    /// binding of its patient tokens declares their `act` claim one and the
    /// token carries it.
    pub(super) fn representative(&self, issuer: &IssuerSettings) -> Option<Actor> {
        // NOTE: Regulation (EU) 2025/327 Art 4(2), RFC 8693 §4.1: an actor is any delegate,
        // so only the issuer's declaration makes it a representative, never the gateway.
        issuer
            .patient
            .as_ref()
            .filter(|patient| patient.act_is_representative)
            .and_then(|_| self.actor())
    }

    /// The current actor the RFC 8693 §4.1 `act` claim names, when the
    /// token carries one: its `iss` and `sub`, each when it is a string and
    /// not empty, and neither for an `act` that is no object.
    ///
    /// A nested `act`, a prior actor, is not read: RFC 8693 §4.1 has the
    /// consumer "only consider the token's top-level claims and the party
    /// identified as the current actor".
    fn actor(&self) -> Option<Actor> {
        match self.act.as_ref()? {
            ActClaim::Claims(claims) => Some(Actor {
                issuer: claims.text("iss"),
                subject: claims.text("sub"),
            }),
            ActClaim::Other(_) => Some(Actor::default()),
        }
    }

    /// The `ihe_iua` extension, when the token carries one.
    fn iua(&self) -> Option<&IheIua> {
        self.extensions.as_ref()?.ihe_iua.as_ref()
    }
}

/// What the IHE IUA extension states that Implementing Regulation (EU)
/// 2026/2099 Annex Tables 1 and 2 also ask for, where the two coincide.
#[derive(Debug, Default)]
pub(super) struct IuaAnnex {
    /// `national_provider_identifier`, the Annex `hp_identifier`.
    pub(super) hp_identifier: Option<String>,
    /// `subject_role`, the Annex `hp_professional_role`.
    pub(super) roles: Vec<Role>,
    /// `subject_organization_id`, the Annex
    /// `healthcare_provider_identifier`.
    pub(super) provider_identifier: Option<String>,
    /// `subject_organization`, the Annex `healthcare_provider_name`.
    pub(super) provider_name: Option<String>,
}

/// Reads `system|code`, or a bare `code`, as a purpose of use; an empty code
/// is none.
fn token(text: &str) -> Option<PurposeOfUse> {
    let (system, code) = match text.rsplit_once('|') {
        Some((system, code)) => (Some(system).filter(|system| !system.is_empty()), code),
        None => (None, text),
    };
    (!code.is_empty()).then(|| PurposeOfUse {
        system: system.map(str::to_owned),
        code: code.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Actor, Declared, PurposeOfUse, token};

    const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

    #[test]
    fn an_iua_coding_and_a_rar_token_are_both_read() {
        let declared: Declared = serde_json::from_str(&format!(
            r#"{{"extensions":{{"ihe_iua":{{"purpose_of_use":[{{"system":"{ACT_REASON}","code":"TREAT"}}],"subject_organization_id":"urn:oid:2.999.7"}}}},
                "authorization_details":[{{"type":"example","purpose_of_use":"{ACT_REASON}|ETREAT"}}]}}"#
        ))
        .unwrap();
        assert_eq!(
            Some(String::from("urn:oid:2.999.7")),
            declared.organisation()
        );
        assert_eq!(
            vec![
                PurposeOfUse {
                    system: Some(ACT_REASON.to_owned()),
                    code: String::from("ETREAT"),
                },
                PurposeOfUse {
                    system: Some(ACT_REASON.to_owned()),
                    code: String::from("TREAT"),
                },
            ],
            declared.purposes()
        );
    }

    // RFC 8693 §4.1: the outermost `act` is the current actor; a nested one is a prior actor.
    #[test]
    fn the_current_actor_is_read_from_the_outermost_act_claim() {
        let declared: Declared = serde_json::from_str(
            r#"{"act":{"sub":"synthetic-actor","iss":"https://actor.example.test","act":{"sub":"prior"}}}"#,
        )
        .unwrap();
        assert_eq!(
            Some(Actor {
                issuer: Some(String::from("https://actor.example.test")),
                subject: Some(String::from("synthetic-actor")),
            }),
            declared.actor()
        );
        let unnamed: Declared = serde_json::from_str(r#"{"act":"synthetic-actor"}"#).unwrap();
        assert_eq!(
            Some(Actor::default()),
            unnamed.actor(),
            "an act that is no object names no one"
        );
        let none: Declared = serde_json::from_str("{}").unwrap();
        assert_eq!(None, none.actor());
    }

    #[test]
    fn a_coding_without_a_code_and_an_empty_token_declare_nothing() {
        let declared: Declared = serde_json::from_str(
            r#"{"extensions":{"ihe_iua":{"purpose_of_use":[{"system":"x"}]}},"authorization_details":[{"purpose_of_use":"x|"}]}"#,
        )
        .unwrap();
        assert_eq!(declared.purposes(), Vec::<PurposeOfUse>::new());
        assert_eq!(None, token(""));
        assert_eq!(
            Some(PurposeOfUse {
                system: None,
                code: String::from("TREAT")
            }),
            token("TREAT")
        );
    }
}
