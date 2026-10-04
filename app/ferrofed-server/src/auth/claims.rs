// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! (master04 §Capabilities). Every other claim is passed over, and the IUA
//! `person_id`, a patient identifier, is never read (§5.4.1, N33).
//!
//! IUA is cited from the Revision 2.5 Trial Implementation supplement
//! vendored at `docs/specs/ihe-iua/IHE_ITI_Suppl_IUA.md`: ITI TF-2 3.71.4.2.2.1
//! (the JSON Web Token Option) and 3.71.4.2.2.1.1 (the JWT IUA extension).

use serde::Deserialize;

use crate::auth::caller::{PurposeOfUse, Stated};

/// The claims of an RFC 9068 access token, as read after its signature, its
/// issuer, its audience and its validity window were verified.
#[derive(Debug, Deserialize)]
pub(super) struct AccessToken {
    /// `iss`.
    iss: String,
    /// `sub`.
    sub: String,
    /// `client_id`.
    client_id: String,
    /// `iat`, required and otherwise unread.
    #[expect(
        dead_code,
        reason = "RFC 9068 §2.2 requires the claim, so it is read to be required"
    )]
    iat: f64,
    /// `jti`, required and otherwise unread.
    #[expect(
        dead_code,
        reason = "RFC 9068 §2.2 requires the claim, so it is read to be required"
    )]
    jti: String,
    /// `scope`.
    #[serde(default)]
    scope: Option<String>,
    /// The SMART on openEHR `ehrId`, the launch context a `patient/` grant
    /// is confined to.
    #[serde(default, rename = "ehrId")]
    ehr_id: Option<String>,
    /// The organisation and the purposes of use.
    #[serde(flatten)]
    declared: Declared,
}

impl AccessToken {
    /// Returns the token's `ehrId` claim, when it carries one.
    pub(super) fn launch_ehr_id(&self) -> Option<String> {
        self.ehr_id.clone()
    }

    /// Returns what the token states about its caller.
    pub(super) fn stated(self) -> Stated {
        Stated {
            issuer: self.iss,
            subject: self.sub,
            client_id: self.client_id,
            organisation: self.declared.organisation(),
            granted: self.scope.unwrap_or_default(),
            purposes: self.declared.purposes(),
        }
    }
}

/// An RFC 7662 §2.2 introspection answer.
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
    /// The SMART on openEHR `ehrId`.
    #[serde(default, rename = "ehrId")]
    pub(super) ehr_id: Option<String>,
    /// The organisation and the purposes of use.
    #[serde(flatten)]
    pub(super) declared: Declared,
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

    /// The `ihe_iua` extension, when the token carries one.
    fn iua(&self) -> Option<&IheIua> {
        self.extensions.as_ref()?.ihe_iua.as_ref()
    }
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
    use super::{Declared, PurposeOfUse, token};

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

    #[test]
    fn a_coding_without_a_code_and_an_empty_token_declare_nothing() {
        let declared: Declared = serde_json::from_str(
            r#"{"extensions":{"ihe_iua":{"purpose_of_use":[{"system":"x"}]}},"authorization_details":[{"purpose_of_use":"x|"}]}"#,
        )
        .unwrap();
        assert!(declared.purposes().is_empty());
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
