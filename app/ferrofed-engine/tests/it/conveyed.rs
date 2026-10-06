// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity every test request conveys: a synthetic verified caller,
//! signed with a key generated once per test process, and the reading of a
//! conveyed token against the published JWK Set (§13.1, N24, N25).

use std::error::Error;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use ferrofed_engine::conveyance::relayed::{HealthProfessional, HealthcareProvider, Relayed, Role};
use ferrofed_engine::conveyance::{
    Acting, AssuranceLevel, Caller, Conveyance, Principal, Professional, Purpose, Signer, TYPE,
    Verification,
};
use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_testkit::oauth;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use secrecy::SecretString;
use serde::Deserialize;

/// The `iss` the test gateway names itself by.
pub(crate) const GATEWAY: &str = "urn:example:ferrofed-under-test";

/// The issuer that vouched for the synthetic caller.
pub(crate) const UPSTREAM: &str = "https://issuer.example.test";

/// The synthetic caller's subject.
pub(crate) const SUBJECT: &str = "clinician-0042";

/// The synthetic caller's organisation.
pub(crate) const ORGANISATION: &str = "urn:oid:2.999.7";

/// The synthetic caller's scopes as granted.
pub(crate) const SCOPE: &str = "user/aql-*.s user/composition-*.cru";

/// The synthetic professional's name (IHE IUA `subject_name`).
pub(crate) const PROFESSIONAL_NAME: &str = "Example Clinician";

/// The synthetic professional's identifier (IHE IUA
/// `national_provider_identifier`).
pub(crate) const PROFESSIONAL_ID: &str = "urn:oid:2.999.7.1|hp-0042";

/// The agency that issued [`PROFESSIONAL_ID`] (Implementing Regulation (EU)
/// 2026/2099 Annex Table 1 `issuing_authority_name`).
pub(crate) const PROFESSIONAL_AUTHORITY: &str = "Example Registration Agency";

/// The code system of the synthetic professional's role (IHE IUA
/// `subject_role`).
pub(crate) const PRACTITIONER_ROLE: &str =
    "http://terminology.hl7.org/CodeSystem/practitioner-role";

/// The HL7 v3 `ActReason` code system the purpose of use is coded in.
pub(crate) const ACT_REASON: &str = "http://terminology.hl7.org/CodeSystem/v3-ActReason";

/// The signer of every test conveyance.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static SIGNER: LazyLock<Arc<Signer>> =
    LazyLock::new(|| Arc::new(signer().expect("a test key pair should generate")));

/// A signer over a fresh ES384 key, naming the gateway [`GATEWAY`].
pub(crate) fn signer() -> Result<Signer, Box<dyn Error>> {
    let key = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    let keys = KeyRing::new(key, None, Duration::ZERO, Arc::new(SystemClock))?;
    Ok(Signer::new(Arc::new(keys), GATEWAY))
}

/// The shared test signer.
pub(crate) fn shared() -> Arc<Signer> {
    Arc::clone(&SIGNER)
}

/// The synthetic caller, verified by signature, treating.
pub(crate) fn caller() -> Caller {
    Caller {
        issuer: UPSTREAM.to_owned(),
        subject: SUBJECT.to_owned(),
        organisation: Some(ORGANISATION.to_owned()),
        purposes: vec![Purpose {
            system: Some(ACT_REASON.to_owned()),
            code: "TREAT".to_owned(),
        }],
        scope: SCOPE.to_owned(),
        verified_by: Verification::Signature,
        professional: Box::new(Professional {
            name: Some(PROFESSIONAL_NAME.to_owned()),
            identifier: Some(PROFESSIONAL_ID.to_owned()),
            issuing_authority: Some(PROFESSIONAL_AUTHORITY.to_owned()),
            roles: vec![Role {
                system: Some(PRACTITIONER_ROLE.to_owned()),
                code: "doctor".to_owned(),
            }],
        }),
        acting: Acting::Person,
        assurance_level: Some(AssuranceLevel::Substantial),
        relayed: None,
    }
}

/// The conveyance of [`caller`] by the shared signer.
pub(crate) fn conveyance() -> Conveyance {
    Conveyance::new(shared(), Principal::Caller(caller()))
}

/// The conveyance of `caller` by the shared signer.
pub(crate) fn conveyance_of(caller: Caller) -> Conveyance {
    Conveyance::new(shared(), Principal::Caller(caller))
}

/// One purpose of use as a node reads it.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPurpose {
    /// The code system.
    pub(crate) system: Option<String>,
    /// The code.
    pub(crate) code: String,
}

/// The claims of a conveyed token as a node reads them; any claim not named
/// here fails the read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Read {
    pub(crate) iss: String,
    pub(crate) aud: String,
    pub(crate) iat: i64,
    pub(crate) exp: i64,
    pub(crate) jti: String,
    pub(crate) sub: String,
    pub(crate) iss_upstream: Option<String>,
    pub(crate) verified_by: Option<String>,
    pub(crate) subject_organization_id: Option<String>,
    #[serde(default)]
    pub(crate) purpose_of_use: Vec<ReadPurpose>,
    pub(crate) scope: Option<String>,
    pub(crate) subject_name: Option<String>,
    pub(crate) national_provider_identifier: Option<String>,
    pub(crate) national_provider_identifier_authority: Option<String>,
    #[serde(default)]
    pub(crate) subject_role: Vec<ReadPurpose>,
    pub(crate) acting: Option<String>,
    pub(crate) assurance_level: Option<String>,
    pub(crate) national_contact_point: Option<ReadContactPoint>,
}

/// The `national_contact_point` claim as a node reads it.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadContactPoint {
    pub(crate) asserted_by: String,
    pub(crate) health_professional: ReadProfessional,
    pub(crate) healthcare_provider: ReadProvider,
}

/// 2026/2099 Annex Table 1 as a node reads it.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadProfessional {
    pub(crate) family_name: String,
    pub(crate) given_name: String,
    pub(crate) country_code: String,
    pub(crate) hp_identifier: String,
    pub(crate) issuing_authority_name: String,
    pub(crate) hp_professional_role: Vec<ReadPurpose>,
    pub(crate) healthcare_provider_identifier: String,
}

/// 2026/2099 Annex Table 2 as a node reads it.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadProvider {
    pub(crate) healthcare_provider_identifier: String,
    pub(crate) issuing_authority_name: String,
    pub(crate) healthcare_provider_name: String,
    pub(crate) healthcare_provider_address: String,
}

/// The synthetic national contact point.
pub(crate) const CONTACT_POINT: &str = "https://ncp.example.test";

/// What the synthetic contact point relays: a professional and a provider
/// of a Member State invented for the test (`XA`, an ISO 3166-1 user-assigned
/// code).
pub(crate) fn relayed() -> Relayed {
    Relayed {
        contact_point: CONTACT_POINT.to_owned(),
        country_code: "XA".to_owned(),
        professional: HealthProfessional {
            family_name: "Example-Family".to_owned(),
            given_name: "Example-Given".to_owned(),
            hp_identifier: "XA-HP-0042".to_owned(),
            issuing_authority_name: "Example Professional Register".to_owned(),
            hp_professional_role: vec![Role {
                system: Some("urn:oid:2.999.9".to_owned()),
                code: "physician".to_owned(),
            }],
        },
        provider: HealthcareProvider {
            identifier: "XA-HCP-0007".to_owned(),
            issuing_authority_name: "Example Provider Register".to_owned(),
            name: "Example Hospital".to_owned(),
            address: "1 Example Street, Example City".to_owned(),
        },
    }
}

/// Verifies `token` as a node does: its `typ`, its algorithm against the
/// one its key is published with, its signature against `keys`'s published
/// JWK Set by `kid`, its `iss`, its `aud` `audience` and its `exp`.
pub(crate) fn verified(
    token: &str,
    keys: &KeyRing,
    (issuer, audience): (&str, &str),
) -> Result<Read, Box<dyn Error>> {
    let header = jsonwebtoken::decode_header(token)?;
    let kid = header.kid.ok_or("the token names its key")?;
    let published = keys.published();
    let jwk = published.find(&kid).ok_or("the key is published")?;
    let algorithm = Algorithm::try_from(
        jwk.common
            .key_algorithm
            .ok_or("the published key names its algorithm")?,
    )?;
    if header.typ.as_deref() != Some(TYPE) || header.alg != algorithm {
        return Err(format!("typ {:?}, alg {:?}", header.typ, header.alg).into());
    }
    let mut validation = Validation::new(algorithm);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    Ok(jsonwebtoken::decode::<Read>(token, &DecodingKey::from_jwk(jwk)?, &validation)?.claims)
}
