// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward lane against a `wiremock` token endpoint: a professional of
//! another domestic party is exchanged at the lane's `federation` issuer, a
//! subject issuer the federation issuer is not trusted to map is refused
//! before anything is sent, and a system is exchanged at no issuer and never
//! carried under the gateway's own token (§13.1, N25; RFC 8693 §2.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::Principal;
use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::exchange::{OnBehalf as _, SubjectToken};
use ferrofed_engine::onward::grant::lane::{Lane, LaneError};
use ferrofed_registry::id::EndpointId;
use ferrofed_testkit::mock::Server;
use ferrohealth_smart::client::onward::{OnwardError, Rejected, SubjectToken as OnwardSubject};
use ferrohealth_smart::config::OauthSection;
use ferrohealth_smart::config::onward::OnwardSection;
use ferrohealth_smart::federation::{Attribute, External, Field};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use ferrohealth_smart::resource::Caller as SmartCaller;
use openehr_its::rest::client::Credentials;
use secrecy::SecretString;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, shared};

type TestResult = Result<(), Box<dyn Error>>;

/// The token endpoint's onward issuer.
const ONWARD_ISSUER: &str = "https://federation.cdr-a.example.org";

/// The node the onward tokens are for.
const NODE: &str = "https://cdr-a.example.org/openehr";

/// The token-exchange grant type, form-encoded (RFC 8693 §2.1).
const TOKEN_EXCHANGE: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange";

/// The gateway's own token, which no caller's call may carry.
const OWN_TOKEN: &str = "synthetic-gateway-own-token";

/// An authorization server refusing every exchange with `invalid_grant`.
async fn refusing() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(TOKEN_EXCHANGE))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(serde_json::json!({"error": "invalid_grant"})),
        )
        .mount(&server)
        .await;
    server
}

/// The lane of a `federation` issuer at `server`, trusted to map the persons
/// of `exchanges_from`.
fn federation_lane(server: &Server, exchanges_from: &str) -> Result<Lane, Box<dyn Error>> {
    let oauth: OauthSection = serde_json::from_value(serde_json::json!({
        "token_endpoint": format!("{}/token", server.uri()),
        "client_id": "ferrofed-onward-gateway",
        "client_auth": "client_secret_post",
        "client_secret": "synthetic-onward-secret",
    }))?;
    let section: OnwardSection = serde_json::from_value(serde_json::json!({
        "issuer": ONWARD_ISSUER,
        "kind": "federation",
        "profile": "rfc9068",
        "token_endpoint": format!("{}/token", server.uri()),
        "audiences": [NODE],
        "indicators": [NODE],
        "keys": {"max_onward_lifetime": 3600},
        "federation": {
            "contact_point": false,
            "exchanges_from": [exchanges_from],
            "person_mapping": "hp_identifier",
            "countries": ["NL"],
            "issuing_authorities": ["synthetic-authority"],
            "provider_identifier_prefixes": ["urn:oid:2.999"],
        },
    }))?;
    let lane = section.resolve(oauth.resolve()?)?;
    let own: SharedCredentials = Arc::new(Credentials::Bearer(SecretString::from(OWN_TOKEN)));
    Ok(Lane::new(
        EndpointId::new("node-a-pub")?,
        (lane.onward, lane.target),
        IssuerKind::Federation,
        own,
    ))
}

/// The conveyance of a caller of `kind` whose verified token `ferrohealth-smart`
/// read from [`UPSTREAM`].
fn conveyance_of(kind: CallerKind) -> Result<Conveyance, Box<dyn Error>> {
    let mut claims = serde_json::Map::new();
    claims.insert("iss".to_owned(), UPSTREAM.into());
    claims.insert("sub".to_owned(), "synthetic-remote-professional".into());
    claims.insert("hp_identifier".to_owned(), "synthetic-hp-0042".into());
    let smart = SmartCaller::new(
        "synthetic-remote-professional".to_owned(),
        Some("synthetic-client".to_owned()),
        "user/aql-*.s",
        None,
        false,
        kind,
    )
    .with_issuer(UPSTREAM.to_owned())
    .with_claims(claims);
    let token = SecretString::from("synthetic-opaque-caller-token");
    let onward = OnwardSubject::from_caller(&smart, token.clone())?;
    Ok(Conveyance::new(shared(), Principal::Caller(caller()))
        .with_subject(SubjectToken::new(token, "user/aql-*.s").with_onward(onward)))
}

/// A professional of another domestic party.
fn external_person() -> CallerKind {
    let fields = BTreeMap::from([(
        Field::ProfessionalHpIdentifier,
        Attribute::Text("synthetic-hp-0042".to_owned()),
    )]);
    CallerKind::ExternalPerson(Box::new(External::new(
        "synthetic-remote-professional",
        fields,
    )))
}

/// Whether the lane refuses a call conveying `conveyance` as `expected`
/// says, with its refusal's account.
async fn refuses(
    lane: &Lane,
    conveyance: &Conveyance,
    expected: impl Fn(&LaneError) -> bool,
) -> Result<(bool, String), Box<dyn Error>> {
    let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));
    let Err(error) = lane.provider(conveyance, &withheld).credentials().await else {
        return Err("the lane gave credentials".into());
    };
    let source = Error::source(&error).ok_or("a cause")?;
    let refused = source
        .downcast_ref::<LaneError>()
        .ok_or("the lane's refusal")?;
    Ok((expected(refused), format!("{refused:?}")))
}

/// The forms the token endpoint received for an exchange.
async fn exchanges(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .filter(|form| form.contains(TOKEN_EXCHANGE))
        .collect())
}

/// A professional of another domestic party is exchanged at the lane's
/// `federation` issuer: its token is the subject, and the issuer's refusal
/// fails the call with the upstream status.
#[tokio::test]
async fn an_external_person_is_exchanged_at_the_federation_issuer() -> TestResult {
    let server = refusing().await;
    let lane = federation_lane(&server, UPSTREAM)?;

    let (refused, account) = refuses(&lane, &conveyance_of(external_person())?, |error| {
        matches!(error, LaneError::Onward(error) if error.status() == Some(http::StatusCode::BAD_REQUEST))
    })
    .await?;
    assert!(refused, "the token endpoint's refusal: {account}");
    let forms = exchanges(&server).await?;
    assert_eq!(1, forms.len(), "{forms:?}");
    let form = forms.first().ok_or("an exchange")?;
    assert!(
        form.contains("subject_token=synthetic-opaque-caller-token"),
        "{form}"
    );
    Ok(())
}

/// A subject whose issuer the `federation` issuer is not trusted to map is
/// refused before anything is sent.
#[tokio::test]
async fn an_unmapped_subject_issuer_is_refused_before_anything_is_sent() -> TestResult {
    let server = refusing().await;
    let lane = federation_lane(&server, "https://another-issuer.example.test")?;

    let (refused, account) = refuses(&lane, &conveyance_of(external_person())?, |error| {
        matches!(
            error,
            LaneError::Onward(OnwardError::Rejected(Rejected::NotMapped))
        )
    })
    .await?;
    assert!(refused, "{account}");
    assert!(exchanges(&server).await?.is_empty(), "nothing was sent");
    Ok(())
}

/// A system originates no request a lane carries: it is refused, never sent
/// under the gateway's own token.
#[tokio::test]
async fn a_system_caller_is_refused_and_never_sent_under_the_gateways_token() -> TestResult {
    let server = refusing().await;
    let lane = federation_lane(&server, UPSTREAM)?;

    let (refused, account) = refuses(&lane, &conveyance_of(CallerKind::System)?, |error| {
        matches!(
            error,
            LaneError::NotCarried {
                issuer: IssuerKind::Federation,
                ..
            }
        )
    })
    .await?;
    assert!(refused, "{account}");
    assert!(exchanges(&server).await?.is_empty(), "nothing was sent");
    Ok(())
}

/// The gateway's own requests, which no caller originates, carry the
/// gateway's own token.
#[tokio::test]
async fn the_gateways_own_request_carries_its_own_token() -> TestResult {
    let server = refusing().await;
    let lane = federation_lane(&server, UPSTREAM)?;
    let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));

    let credentials = lane
        .provider(&Conveyance::new(shared(), Principal::Gateway), &withheld)
        .credentials()
        .await?;
    let Credentials::Bearer(token) = credentials else {
        return Err("a bearer token".into());
    };
    assert_eq!(OWN_TOKEN, secrecy::ExposeSecret::expose_secret(&token));
    Ok(())
}
