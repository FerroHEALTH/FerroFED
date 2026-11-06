// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national-contact-point caller: a `federation` issuer whose
//! `[[auth.federation]]` table sets `contact_point = true`, whose connector
//! relays the request of a health professional of another Member State
//! (Regulation (EU) 2025/327 Art 23; Implementing Regulation (EU) 2026/2099
//! Art 6 and 7).
//!
//! `ferrohealth-smart` holds its token to the MyHealth@EU contact-point
//! profile (eHDSI SAML Profile 10.0.0 §2.1, §2.3), and a token that falls
//! short is refused with nothing sent ([`admission`]); its requests are
//! served with consent exclusions withheld whatever the deployment's
//! setting (Regulation (EU) 2025/327 Art 8, Art 11(5); [`withheld`]); what
//! it relays reaches the node and the access record marked as asserted by
//! the contact point, with no name or address, and no IUA `person_id`
//! reaches a node (§13.4 authn-end-user, N24, N33; [`relayed`]);
//! `OPTIONS {base}/` declares the setting ([`declared`]); an emergency
//! access it relays is held to the professional's limit (eHDSI Section II
//! Security Services 10.0.0 §2.5; [`emergency`]); and its requests on the
//! ITS-REST face are held to the patient of its treatment relationship
//! confirmation (eHDSI XCA Profile 8.0.1 §4.2; [`trc`]).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the eHDSI attributes a test token asserts are written as JSON"
)]

mod admission;
mod config;
mod declared;
pub(crate) mod emergency;
mod relayed;
mod trc;
mod withheld;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::{Arc, LazyLock};

use axum::body::Body;
use ferrofed_server::config::auth::contact_point::ContactPoint;
use ferrofed_server::config::auth::{AuthSettings, IssuerSettings, Verification};
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::national_connector;
use ferrohealth_server::tls::PeerCertificates;
use ferrohealth_smart::federation::{FederationSection, Field, Mapping, Source};
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use ferrohealth_smart::sender::SenderConstraint;
use http::{HeaderName, Request};
use rustls::pki_types::CertificateDer;
use serde_json::{Value, json};

use crate::auth::{bearing, minted};
use crate::support;
use crate::support::caller::AUDIENCE;

/// The issuer of the test contact point's tokens, which signs with the
/// suite's test key.
pub(crate) const NCP_ISSUER: &str = "https://ncp.issuer.example.test";

/// The header the test connector sends its correlation identifier in.
pub(crate) const CORRELATION_HEADER: &str = "ncp-correlation-id";

/// A synthetic correlation identifier.
pub(crate) const CORRELATION: &str = "Qz7-ncp-exchange-0815";

/// A synthetic IHE IUA `person_id`, the patient identifier a contact point's
/// token may carry and the gateway never reads.
pub(crate) const PERSON_ID: &str = "SENTINEL-PERSON-ID-7q3z";

/// The synthetic attribute values of the test contact point, each unlike
/// any other text: the configured country and issuing authorities, and
/// what the identity assertion carries, the professional's and the
/// provider's names included, which the gateway never relays.
pub(crate) const COUNTRY: &str = "LU";
pub(crate) const HP_ID: &str = "Qz7-hp-63";
pub(crate) const HP_NAME: &str = "Qz7-hp-name-61";
pub(crate) const HP_AUTHORITY: &str = "Qz7-hp-authority-64";
pub(crate) const ROLE_SYSTEM: &str = "urn:oid:2.999.9";
pub(crate) const ROLE: &str = "Qz7-role-65";
pub(crate) const HCP_ID: &str = "urn:oid:2.999.9.66";
pub(crate) const HCP_AUTHORITY: &str = "Qz7-hcp-authority-67";
pub(crate) const HCP_NAME: &str = "Qz7-hcp-name-68";

/// The client certificate of the test connector's connection, made once per
/// test process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot make a certificate cannot test anything"
)]
static CERTIFICATE: LazyLock<Vec<u8>> = LazyLock::new(|| {
    national_connector::client_certificate().expect("a test certificate should be made")
});

/// The `[[auth.federation]]` table of the test contact point: the `ehdsi`
/// mapping, with the country and the issuing authorities configured.
pub(crate) fn federation() -> FederationSection {
    let constant = |value: &str| vec![Source::Constant(value.to_owned())];
    FederationSection {
        issuer: NCP_ISSUER.to_owned(),
        contact_point: Some(true),
        mapping: Some(Mapping::Ehdsi),
        fields: BTreeMap::from([
            (Field::ProfessionalCountryCode, constant(COUNTRY)),
            (
                Field::ProfessionalIssuingAuthorityName,
                constant(HP_AUTHORITY),
            ),
            (Field::ProviderIssuingAuthorityName, constant(HCP_AUTHORITY)),
        ]),
        countries: vec![COUNTRY.to_owned()],
        ..FederationSection::default()
    }
}

/// The test contact point: a `federation` issuer on the suite's key set,
/// its tokens bound to a certificate, declared with [`CORRELATION_HEADER`],
/// its connector's client a demographic client as the suite's default
/// client is.
pub(crate) fn contact_point() -> IssuerSettings {
    let mut settings = KeySetSettings::new(NCP_ISSUER, AUDIENCE, IssuerKind::Federation);
    settings.jwks_file = Some(support::caller::jwks_file().to_owned());
    settings.sender_constraint = SenderConstraint::Required;
    settings.federation = Some(federation());
    let mut issuer = IssuerSettings::new(NCP_ISSUER, Verification::KeySet(Box::new(settings)));
    issuer.national_contact_point = Some(ContactPoint {
        correlation_header: Some(HeaderName::from_static(CORRELATION_HEADER)),
        additional_countries: BTreeSet::new(),
    });
    issuer.demographic_clients = BTreeSet::from([relaying().client_id]);
    issuer
}

/// The suite's `[auth]`, with the test contact point trusted beside the
/// test issuer.
pub(crate) fn declared() -> AuthSettings {
    let mut auth = support::caller::auth();
    auth.issuers.push(contact_point());
    auth
}

/// The claims of the test connector's token: a client token, its `sub` its
/// `client_id`, what a conforming eHDSI assertion provides with the purpose
/// of use `TREATMENT`, the TRC patient the facade suite's synthetic patient,
/// bound to [`CERTIFICATE`], and an IUA `person_id`.
pub(crate) fn relaying() -> Claims {
    let mut claims = Claims::new(NCP_ISSUER, AUDIENCE);
    claims.sub.clone_from(&claims.client_id);
    if let Some(extensions) = claims.extensions.as_mut() {
        extensions.ihe_iua.person_id = Some(PERSON_ID.to_owned());
    }
    claims.ehdsi = national_connector::asserted(HP_ID, "TREATMENT", claims.iat);
    for (field, value) in [
        ("professional.name", json!(HP_NAME)),
        (
            "professional.hp_professional_role",
            json!([{ "system": ROLE_SYSTEM, "code": ROLE }]),
        ),
        ("provider.healthcare_provider_identifier", json!(HCP_ID)),
        ("provider.healthcare_provider_name", json!(HCP_NAME)),
        // NOTE: eHDSI SAML Profile 10.0.0 §4.3: the TRC's subject-id, here the facade suite's
        // synthetic patient, as the contact point's authorization server writes it.
        (
            "patient.id",
            json!(format!(
                "{}|{}",
                crate::facade::NAMESPACE,
                crate::facade::PATIENT
            )),
        ),
    ] {
        claims.ehdsi.insert(field.to_owned(), value);
    }
    claims.cnf = Some(national_connector::cnf(&CERTIFICATE));
    claims
}

/// The claims of [`relaying`] with `field` removed.
pub(crate) fn without(field: &str) -> Claims {
    let mut claims = relaying();
    claims.ehdsi.remove(field);
    claims
}

/// The claims of [`relaying`] with `field` set to `value`.
pub(crate) fn with(field: &str, value: Value) -> Claims {
    let mut claims = relaying();
    claims.ehdsi.insert(field.to_owned(), value);
    claims
}

/// `request` as the test connector sends it: bearing `token`, on a
/// connection whose client presented [`CERTIFICATE`], as the listener's
/// TLS layer hands it over.
pub(crate) fn bound(request: Request<Body>, token: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = bearing(request, token)?;
    request
        .extensions_mut()
        .insert(PeerCertificates(Arc::from(vec![CertificateDer::from(
            CERTIFICATE.clone(),
        )])));
    Ok(request)
}

/// `request` as the test connector sends it, bearing a token over `claims`
/// ([`bound`]).
pub(crate) fn presenting(
    request: Request<Body>,
    claims: &Claims,
) -> Result<Request<Body>, Box<dyn Error>> {
    bound(request, &minted(claims)?)
}

/// The claims of the conveyance the first request `node` received carries,
/// read without verifying it.
pub(crate) async fn told(
    node: &ferrofed_testkit::mock::Server,
) -> Result<support::conveyed::Conveyed, Box<dyn Error>> {
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let request = requests.first().ok_or("the node was asked")?;
    let token = request
        .headers
        .get(ferrofed_engine::conveyance::HEADER)
        .ok_or("the conveyance")?
        .to_str()?;
    support::conveyed::conveyed_claims(token)
}
