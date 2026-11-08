// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward lane under each example profile, and the onward check on each
//! route: a person of the node's organisation is exchanged under the
//! standards-only profile, Keycloak's standard exchange and Entra ID's
//! on-behalf-of grant, and an issued token that names another person, comes
//! from another authorization server or maps another professional is
//! refused before the node is sent anything (§13.1, N25; RFC 8693 §2.1,
//! RFC 7523 §2.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::Principal;
use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::exchange::{OnBehalf as _, SubjectToken};
use ferrofed_engine::onward::grant::lane::{Lane, LaneError, Organisations};
use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::mock::Server;
use ferrohealth_smart::client::onward::{OnwardError, Rejected, SubjectToken as OnwardSubject};
use ferrohealth_smart::config::OauthSection;
use ferrohealth_smart::config::onward::OnwardSection;
use ferrohealth_smart::federation::{Attribute, External, Field};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use ferrohealth_smart::resource::Caller as SmartCaller;
use openehr_its::rest::client::Credentials;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{caller, shared};
use crate::lane::{NODE, OWN_TOKEN, refuses};

type TestResult = Result<(), Box<dyn Error>>;

/// The node's `persons` issuer, which also issued the caller's token.
pub(crate) const PERSONS: &str = "https://as.cdr-a.example.org";

/// The node's `federation` issuer.
pub(crate) const FEDERATION: &str = "https://federation.cdr-a.example.org";

/// The issuer of another domestic party's professionals.
pub(crate) const PARTNER: &str = "https://as.partner.example.org";

/// The gateway's client at the onward issuer.
pub(crate) const CLIENT_ID: &str = "ferrofed-onward-gateway";

/// The caller, a person of the node's organisation.
pub(crate) const PERSON: &str = "synthetic-clinician-0042";

/// The caller's token as the token endpoint receives it.
pub(crate) const CALLER_TOKEN: &str = "synthetic-caller-token";

/// The professional the partner's token names, the mapped claim.
pub(crate) const HP_IDENTIFIER: &str = "synthetic-hp-0042";

/// The seconds since the Unix epoch.
pub(crate) fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// `claims` as an unsigned compact JWS, which the onward check decodes.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) fn jws(claims: &serde_json::Value) -> String {
    format!("e30.{}.c2ln", URL_SAFE_NO_PAD.encode(claims.to_string()))
}

/// A token endpoint answering every request with `issued`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) async fn issuing(issued: &serde_json::Value) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": jws(issued),
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    server
}

/// The forms the token endpoint received.
pub(crate) async fn forms(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect())
}

/// The lane of `section`'s issuer of `kind` at `server`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn lane(
    server: &Server,
    kind: IssuerKind,
    section: serde_json::Value,
) -> Result<Lane, Box<dyn Error>> {
    lane_carrying(server, kind, section, Organisations::Refused)
}

/// The lane of `section`'s issuer of `kind` at `server`, carrying
/// `organisations`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) fn lane_carrying(
    server: &Server,
    kind: IssuerKind,
    mut section: serde_json::Value,
    organisations: Organisations,
) -> Result<Lane, Box<dyn Error>> {
    let oauth: OauthSection = serde_json::from_value(json!({
        "token_endpoint": format!("{}/token", server.uri()),
        "client_id": CLIENT_ID,
        "client_auth": "client_secret_post",
        "client_secret": "synthetic-onward-secret",
    }))?;
    if let Some(object) = section.as_object_mut() {
        object.insert(
            "token_endpoint".to_owned(),
            format!("{}/token", server.uri()).into(),
        );
        object.insert("audiences".to_owned(), json!([NODE]));
        let keys = object
            .entry("keys")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("keys are a table")?;
        keys.insert("max_onward_lifetime".to_owned(), 3600.into());
    }
    let section: OnwardSection = serde_json::from_value(section)?;
    let resolved = section.resolve(oauth.resolve()?)?;
    let own: SharedCredentials = Arc::new(Credentials::Bearer(SecretString::from(OWN_TOKEN)));
    Ok(Lane::new(
        EndpointId::new("node-a-pub")?,
        (resolved.onward, resolved.target),
        (kind, organisations),
        own,
    ))
}

/// The lane of the node's `persons` issuer under `profile`, with `extra`
/// onward keys.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn persons_lane(
    server: &Server,
    profile: &str,
    extra: serde_json::Value,
) -> Result<Lane, Box<dyn Error>> {
    let mut section = json!({"issuer": PERSONS, "kind": "persons", "profile": profile});
    if let (Some(object), serde_json::Value::Object(extra)) = (section.as_object_mut(), extra) {
        object.extend(extra);
    }
    lane(server, IssuerKind::Persons, section)
}

/// The conveyance of a caller of `kind` whose verified token, issued by
/// `issuer`, carries `claims`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) fn conveyance(
    issuer: &str,
    kind: CallerKind,
    claims: &serde_json::Value,
) -> Result<Conveyance, Box<dyn Error>> {
    let claims = claims.as_object().cloned().ok_or("claims are an object")?;
    let smart = SmartCaller::new(
        PERSON.to_owned(),
        Some("synthetic-client".to_owned()),
        "user/aql-*.s",
        None,
        false,
        kind,
    )
    .with_issuer(issuer.to_owned())
    .with_claims(claims);
    let token = SecretString::from(CALLER_TOKEN);
    let onward = OnwardSubject::from_caller(&smart, token.clone())?;
    Ok(Conveyance::new(shared(), Principal::Caller(caller()))
        .with_subject(SubjectToken::new(token, "user/aql-*.s").with_onward(onward)))
}

/// The claims of the person's token at the node's `persons` issuer, naming
/// `audience`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn person_claims(audience: &[&str]) -> serde_json::Value {
    json!({
        "iss": PERSONS, "sub": PERSON, "aud": audience, "client_id": "synthetic-client",
        "azp": "synthetic-client", "oid": "synthetic-oid-0042", "tid": "synthetic-tenant",
        "iat": now(), "exp": now() + 600, "jti": "synthetic-subject-1",
    })
}

/// The token the `persons` issuer issues for the person to the gateway.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
fn issued_for(person: &str) -> serde_json::Value {
    json!({
        "iss": PERSONS, "sub": person, "aud": NODE, "client_id": CLIENT_ID, "azp": CLIENT_ID,
        "oid": "synthetic-oid-0042", "tid": "synthetic-tenant", "ver": "2.0",
        "iat": now(), "exp": now() + 300, "jti": "synthetic-issued-1",
    })
}

/// The bearer token the lane gives a call conveying `conveyance`.
pub(crate) async fn sent(lane: &Lane, conveyance: &Conveyance) -> Result<String, Box<dyn Error>> {
    let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));
    match lane.provider(conveyance, &withheld).credentials().await? {
        Credentials::Bearer(token) => Ok(token.expose_secret().to_owned()),
        _ => Err("a bearer token".into()),
    }
}

/// The standards-only profile exchanges the caller's token for the node
/// as its RFC 8707 resource, and the issued token naming the same person
/// and the gateway as its client is what the node is sent.
#[tokio::test]
async fn the_standards_profile_exchanges_the_person_for_the_node_as_its_resource() -> TestResult {
    let issued = issued_for(PERSON);
    let server = issuing(&issued).await;
    let lane = persons_lane(&server, "rfc9068", json!({"indicators": [NODE]}))?;
    let call = conveyance(PERSONS, CallerKind::Person, &person_claims(&[NODE]))?;

    assert_eq!(jws(&issued), sent(&lane, &call).await?);
    let forms = forms(&server).await?;
    let form = forms.first().ok_or("an exchange")?;
    assert!(
        form.contains(&format!("subject_token={CALLER_TOKEN}")),
        "{form}"
    );
    assert!(
        form.contains("resource=https%3A%2F%2Fcdr-a.example.org%2Fopenehr"),
        "{form}"
    );
    Ok(())
}

/// What a Keycloak issuer's table sets beside its preset: the node's client
/// as the `audience` indicator, and its service accounts' subjects, which
/// the preset leaves for the deployment to list.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) fn keycloak_keys() -> serde_json::Value {
    json!({
        "indicators": [NODE],
        "keys": {"app_only": [{"subjects": ["service-account-ferrofed"]}]},
    })
}

/// Keycloak's standard exchange names the node's client in `audience`, and
/// the issued token names the gateway in `azp` with no `act`.
#[tokio::test]
async fn keycloak_exchanges_the_person_for_the_node_as_its_audience() -> TestResult {
    let issued = issued_for(PERSON);
    let server = issuing(&issued).await;
    let lane = persons_lane(&server, "keycloak", keycloak_keys())?;
    let call = conveyance(
        PERSONS,
        CallerKind::Person,
        &person_claims(&[NODE, CLIENT_ID]),
    )?;

    assert_eq!(jws(&issued), sent(&lane, &call).await?);
    let forms = forms(&server).await?;
    let form = forms.first().ok_or("an exchange")?;
    assert!(
        form.contains("audience=https%3A%2F%2Fcdr-a.example.org%2Fopenehr"),
        "{form}"
    );
    assert!(!form.contains("resource="), "{form}");
    Ok(())
}

/// Under Keycloak, a caller's token that does not name the gateway's client
/// in `aud`, which the inbound client scope's audience mapper adds, is
/// refused before the token endpoint is asked.
#[tokio::test]
async fn keycloak_refuses_a_caller_token_without_the_gateway_in_its_audience() -> TestResult {
    let server = issuing(&issued_for(PERSON)).await;
    let lane = persons_lane(&server, "keycloak", keycloak_keys())?;
    let call = conveyance(PERSONS, CallerKind::Person, &person_claims(&[NODE]))?;

    let (refused, account) = refuses(&lane, &call, |error| {
        matches!(error, LaneError::Onward(OnwardError::SubjectAudience))
    })
    .await?;
    assert!(refused, "{account}");
    assert!(forms(&server).await?.is_empty(), "nothing was sent");
    Ok(())
}

/// Entra ID's on-behalf-of grant sends the caller's token as the JWT bearer
/// assertion with the node's scope, and the issued token naming the same
/// `oid` and `tid` is what the node is sent.
#[tokio::test]
async fn entra_exchanges_the_person_on_behalf_of() -> TestResult {
    let issued = issued_for(PERSON);
    let server = issuing(&issued).await;
    let lane = persons_lane(
        &server,
        "entra",
        json!({"scope": "api://cdr-a/user_impersonation"}),
    )?;
    let call = conveyance(
        PERSONS,
        CallerKind::Person,
        &person_claims(&[NODE, CLIENT_ID]),
    )?;

    assert_eq!(jws(&issued), sent(&lane, &call).await?);
    let forms = forms(&server).await?;
    let form = forms.first().ok_or("an exchange")?;
    assert!(
        form.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"),
        "{form}"
    );
    assert!(form.contains("requested_token_use=on_behalf_of"), "{form}");
    assert!(
        form.contains(&format!("assertion={CALLER_TOKEN}")),
        "{form}"
    );
    Ok(())
}

/// A same-organisation exchange answered with a token for another person
/// is refused before use, so the node is sent nothing.
#[tokio::test]
async fn a_token_for_another_person_is_refused_before_use() -> TestResult {
    let server = issuing(&issued_for("synthetic-clinician-0099")).await;
    let lane = persons_lane(&server, "rfc9068", json!({"indicators": [NODE]}))?;
    let call = conveyance(PERSONS, CallerKind::Person, &person_claims(&[NODE]))?;

    let (refused, account) = refuses(&lane, &call, |error| {
        matches!(
            error,
            LaneError::Onward(OnwardError::Rejected(Rejected::Person { claim })) if claim == "sub"
        )
    })
    .await?;
    assert!(refused, "{account}");
    Ok(())
}

/// A token another authorization server issued is refused before use.
#[tokio::test]
async fn a_token_from_another_authorization_server_is_refused_before_use() -> TestResult {
    let mut issued = issued_for(PERSON);
    issued["iss"] = json!("https://as.elsewhere.example.org");
    let server = issuing(&issued).await;
    let lane = persons_lane(&server, "rfc9068", json!({"indicators": [NODE]}))?;
    let call = conveyance(PERSONS, CallerKind::Person, &person_claims(&[NODE]))?;

    let (refused, account) = refuses(&lane, &call, |error| {
        matches!(
            error,
            LaneError::Onward(OnwardError::Rejected(Rejected::Issuer))
        )
    })
    .await?;
    assert!(refused, "{account}");
    Ok(())
}

/// The lane of the node's `federation` issuer, which maps the partner's
/// professionals by their `hp_identifier` claim and reads its own tokens
/// as IHE IUA.
fn federation_lane(server: &Server) -> Result<Lane, Box<dyn Error>> {
    let section = json!({
        "issuer": FEDERATION, "kind": "federation", "profile": "rfc9068",
        "indicators": [NODE],
        "federation": {
            "contact_point": false,
            "mapping": "iua",
            "exchanges_from": [PARTNER],
            "person_mapping": "hp_identifier",
            "countries": ["NL"],
            "issuing_authorities": ["synthetic-authority"],
            "provider_identifier_prefixes": ["urn:oid:2.999."],
            "fields": {
                "professional.country_code": [{"constant": "NL"}],
                "professional.issuing_authority_name": [{"constant": "synthetic-authority"}],
            },
        },
        "assurance": {"levels": {"urn:acr:high": "high"}},
    });
    lane(server, IssuerKind::Federation, section)
}

/// The token the `federation` issuer issues, naming the professional
/// `professional`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims and tables a test token and lane carry are written as JSON"
)]
pub(crate) fn federation_token(professional: &str) -> serde_json::Value {
    json!({
        "iss": FEDERATION, "sub": "synthetic-mapped-0042", "client_id": CLIENT_ID,
        "aud": NODE, "jti": "synthetic-issued-2", "iat": now(), "exp": now() + 300,
        "acr": "urn:acr:high",
        "extensions": {"ihe_iua": {
            "subject_name": "synthetic-professional-name",
            "subject_organization": "synthetic-organisation-name",
            "subject_organization_id": "urn:oid:2.999.12345678",
            "national_provider_identifier": professional,
            "subject_role": [{
                "system": "http://terminology.hl7.org/CodeSystem/practitioner-role",
                "code": "doctor",
            }],
            "purpose_of_use": [{
                "system": "http://terminology.hl7.org/CodeSystem/v3-ActReason",
                "code": "TREAT",
            }],
        }},
    })
}

/// The partner's professional, whose token names [`HP_IDENTIFIER`].
pub(crate) fn partner_call() -> Result<Conveyance, Box<dyn Error>> {
    let fields = BTreeMap::from([(
        Field::ProfessionalHpIdentifier,
        Attribute::Text(HP_IDENTIFIER.to_owned()),
    )]);
    let kind = CallerKind::ExternalPerson(Box::new(External::new(PERSON, fields)));
    let claims = json!({
        "iss": PARTNER, "sub": PERSON, "aud": CLIENT_ID, "client_id": "synthetic-client",
        "hp_identifier": HP_IDENTIFIER, "iat": now(), "exp": now() + 600,
    });
    conveyance(PARTNER, kind, &claims)
}

/// A domestic federation exchange answered with the mapped professional is
/// what the node is sent.
#[tokio::test]
async fn a_federation_token_for_the_mapped_professional_is_sent() -> TestResult {
    let issued = federation_token(HP_IDENTIFIER);
    let server = issuing(&issued).await;
    let lane = federation_lane(&server)?;

    assert_eq!(jws(&issued), sent(&lane, &partner_call()?).await?);
    Ok(())
}

/// A domestic federation exchange answered with a token naming another
/// professional than the mapped subject claim is refused before use.
#[tokio::test]
async fn a_federation_token_for_another_professional_is_refused_before_use() -> TestResult {
    let server = issuing(&federation_token("synthetic-hp-0099")).await;
    let lane = federation_lane(&server)?;

    let (refused, account) = refuses(&lane, &partner_call()?, |error| {
        matches!(
            error,
            LaneError::Onward(OnwardError::Rejected(Rejected::PersonMapping))
        )
    })
    .await?;
    assert!(refused, "{account}");
    Ok(())
}
