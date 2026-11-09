// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Another party's organisation through a `federation` lane: refused unless
//! the lane opts in, exchanged when it does, refused every write before any
//! token is asked for, and, on a chained route, refused before sending when
//! its token carries no correlation claim, with the issued token held to
//! carry it (§13.1, N25; RFC 8693 §2.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::Principal;
use ferrofed_engine::dispatch::definition::DefinitionAt;
use ferrofed_engine::dispatch::{DispatchError, DispatchOptions, NodeClient};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::lane::{Lane, LaneError, Organisations};
use ferrofed_engine::onward::grant::on_behalf::{OnBehalf as _, SubjectToken};
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use ferrohealth_smart::client::onward::{OnwardError, Rejected, SubjectToken as OnwardSubject};
use ferrohealth_smart::federation::{Attribute, External, Field};
use ferrohealth_smart::kind::{CallerKind, IssuerKind};
use ferrohealth_smart::resource::Caller as SmartCaller;
use http::{HeaderMap, Method};
use openehr_its::rest::client::{Credentials, ReqwestTransport};
use secrecy::{ExposeSecret as _, SecretString};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, shared};
use crate::lane::{NODE, ONWARD_ISSUER, TOKEN_EXCHANGE, exchanges, lane_of, refuses, refusing};

type TestResult = Result<(), Box<dyn Error>>;

/// The client the onward issuer registered the gateway as.
const CLIENT_ID: &str = "ferrofed-onward-gateway";

/// The other party's organisation, as its issuer names it.
const ORGANISATION: &str = "synthetic-remote-organisation";

/// The correlation claim of the chained route, and the requester's value.
const CLAIM: &str = "txn";
const CORRELATION: &str = "synthetic-correlation-0815";

/// An EHR of node A.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The seconds since the Unix epoch.
fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// `claims` as an unsigned compact JWS, which the onward check decodes.
fn jws(claims: &str) -> String {
    format!("e30.{}.c2ln", URL_SAFE_NO_PAD.encode(claims))
}

/// The claims of the organisation's token at [`UPSTREAM`], carrying the
/// correlation claim when `correlated`.
#[expect(
    clippy::disallowed_types,
    reason = "the claims a test token carries are written as JSON"
)]
fn organisation_claims(correlated: bool) -> serde_json::Map<String, serde_json::Value> {
    let mut claims = serde_json::Map::new();
    claims.insert("iss".to_owned(), UPSTREAM.into());
    claims.insert("sub".to_owned(), ORGANISATION.into());
    claims.insert("client_id".to_owned(), ORGANISATION.into());
    if correlated {
        claims.insert(CLAIM.to_owned(), CORRELATION.into());
    }
    claims
}

/// The organisation's verified caller.
fn organisation() -> CallerKind {
    let fields = BTreeMap::from([(
        Field::ProviderHealthcareProviderIdentifier,
        Attribute::Text("urn:oid:2.999.7".to_owned()),
    )]);
    CallerKind::ExternalOrganisation(Box::new(External::new(ORGANISATION, fields)))
}

/// The conveyance of the organisation, its token a JWS of its claims or,
/// when `opaque`, an opaque token whose introspected claims they are.
fn organisation_conveyance(correlated: bool, opaque: bool) -> Result<Conveyance, Box<dyn Error>> {
    let claims = organisation_claims(correlated);
    let token = if opaque {
        String::from("synthetic-opaque-organisation-token")
    } else {
        jws(&serde_json::to_string(&claims)?)
    };
    let smart = SmartCaller::new(
        ORGANISATION.to_owned(),
        Some(ORGANISATION.to_owned()),
        "system/aql-*.s",
        None,
        false,
        organisation(),
    )
    .with_issuer(UPSTREAM.to_owned())
    .with_claims(claims);
    let token = SecretString::from(token);
    let onward = OnwardSubject::from_caller(&smart, token.clone())?;
    Ok(Conveyance::new(shared(), Principal::Caller(caller()))
        .with_subject(SubjectToken::new(token, "system/aql-*.s").with_onward(onward)))
}

/// The organisation token the onward issuer issues, carrying the
/// correlation claim when `correlated`.
fn issued(correlated: bool) -> Result<String, Box<dyn Error>> {
    let now = now();
    let mut claims = serde_json::json!({
        "iss": ONWARD_ISSUER, "sub": CLIENT_ID, "client_id": CLIENT_ID, "aud": NODE,
        "jti": "synthetic-issued-1", "iat": now, "exp": now + 300,
        "organisation_id": "urn:oid:2.999.7",
        "authorization_details": [{"type": "organisation_access", "locations": [NODE]}],
    });
    if correlated && let Some(object) = claims.as_object_mut() {
        object.insert(CLAIM.to_owned(), CORRELATION.into());
    }
    Ok(jws(&serde_json::to_string(&claims)?))
}

/// An authorization server issuing `token` for every exchange.
async fn issuing(token: &str) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(TOKEN_EXCHANGE))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": token,
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    server
}

/// The credentials the lane gives a call conveying `conveyance`.
async fn credentials(lane: &Lane, conveyance: &Conveyance) -> Result<Credentials, Box<dyn Error>> {
    let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));
    Ok(lane.provider(conveyance, &withheld).credentials().await?)
}

/// An organisation is refused by a lane that does not opt in, before any
/// token is asked for.
#[tokio::test]
async fn an_organisation_is_refused_unless_the_lane_opts_in() -> TestResult {
    let server = refusing().await;
    let lane = lane_of(&server, UPSTREAM, Organisations::Refused, None)?;

    let (refused, account) = refuses(&lane, &organisation_conveyance(false, false)?, |error| {
        matches!(
            error,
            LaneError::NotCarried {
                caller: "external_organisation",
                issuer: IssuerKind::Federation,
            }
        )
    })
    .await?;
    assert!(refused, "{account}");
    assert!(exchanges(&server).await?.is_empty(), "nothing was sent");
    Ok(())
}

/// A lane that opts in exchanges the organisation's token at its
/// `federation` issuer and sends the node the issued token.
#[tokio::test]
async fn an_organisation_is_exchanged_at_a_lane_that_opts_in() -> TestResult {
    let token = issued(false)?;
    let server = issuing(&token).await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, None)?;

    let Credentials::Bearer(sent) =
        credentials(&lane, &organisation_conveyance(false, false)?).await?
    else {
        return Err("a bearer token".into());
    };
    assert_eq!(token, sent.expose_secret());
    assert_eq!(1, exchanges(&server).await?.len());
    Ok(())
}

/// The organisation is read-only through the lane; a person is not.
#[tokio::test]
async fn an_organisation_alone_is_read_only() -> TestResult {
    let server = refusing().await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, None)?;
    assert!(lane.read_only(&organisation_conveyance(false, false)?));
    assert!(!lane.read_only(&crate::conveyed::conveyance()));
    assert!(!lane.read_only(&Conveyance::new(shared(), Principal::Gateway)));
    Ok(())
}

/// The client of node A at `url`, reached through `lane`.
fn client(url: &str, lane: Lane) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let snapshot = RegistrySnapshot::from_toml_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    ))?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?
    .with_on_behalf(Arc::new(lane)))
}

/// The options of a call conveying `conveyance`.
fn options(conveyance: Conveyance) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline, conveyance))
}

/// Every write of an organisation is refused with nothing sent and no
/// token asked for: a forwarded `CONTRIBUTION` and a stored query's
/// definition.
#[tokio::test]
async fn an_organisations_write_is_refused_before_any_token_is_asked_for() -> TestResult {
    let server = issuing(&issued(false)?).await;
    let node = Server::start().await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, None)?;
    let client = client(&node.uri(), lane)?;
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse()?);
    let commit = ClientRequest {
        method: Method::POST,
        path: format!("/ehr/{EHR}/contribution"),
        query: None,
        headers,
        body: br#"{"versions":[],"audit":{}}"#.to_vec(),
    };

    let forwarded = client
        .forward(commit, &options(organisation_conveyance(false, false)?)?)
        .await;
    assert!(
        matches!(forwarded, Err(ForwardError::ReadOnly(_))),
        "{forwarded:?}"
    );
    let at = DefinitionAt {
        name: "org.example::synthetic",
        version: "1.0.0",
    };
    let stored = client
        .store_definition(
            at,
            "SELECT c FROM COMPOSITION c",
            &options(organisation_conveyance(false, false)?)?,
        )
        .await;
    assert!(
        matches!(stored, Err(DispatchError::ReadOnly(_))),
        "{stored:?}"
    );
    let asked = server.received_requests().await.ok_or("recording is on")?;
    assert!(asked.is_empty(), "no token was asked for");
    let sent = node.received_requests().await.ok_or("recording is on")?;
    assert!(sent.is_empty(), "node A was sent nothing");
    Ok(())
}

/// A lane with a correlation claim refuses, before the token endpoint is
/// asked, a JWT and an opaque token whose verified claims do not carry the
/// claim (`ferrohealth-smart`'s onward client).
#[tokio::test]
async fn a_chained_lane_refuses_a_token_without_the_correlation_claim() -> TestResult {
    let server = issuing(&issued(true)?).await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, Some(CLAIM))?;
    for conveyance in [
        organisation_conveyance(false, true)?,
        organisation_conveyance(false, false)?,
    ] {
        let (refused, account) = refuses(&lane, &conveyance, |error| {
            matches!(error, LaneError::Onward(OnwardError::NoCorrelation))
        })
        .await?;
        assert!(refused, "{account}");
    }
    assert!(exchanges(&server).await?.is_empty(), "nothing was sent");
    Ok(())
}

/// An opaque token whose introspected claims carry the correlation claim is
/// exchanged on a chained route: the claim is attested by its issuer's
/// introspection answer (RFC 7662 §2.2), which the gateway verified.
#[tokio::test]
async fn an_opaque_token_whose_verified_claims_carry_the_correlation_claim_is_exchanged()
-> TestResult {
    let carried = issued(true)?;
    let server = issuing(&carried).await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, Some(CLAIM))?;
    let Credentials::Bearer(sent) =
        credentials(&lane, &organisation_conveyance(true, true)?).await?
    else {
        return Err("a bearer token".into());
    };
    assert_eq!(carried, sent.expose_secret());
    assert_eq!(1, exchanges(&server).await?.len(), "one exchange was sent");
    Ok(())
}

/// On a chained route the issued token is held to carry the caller's
/// correlation claim: one that carries it is sent, one that does not fails
/// the call (`ferrohealth-smart`'s onward check).
#[tokio::test]
async fn the_issued_token_is_held_to_carry_the_correlation_claim() -> TestResult {
    let carried = issued(true)?;
    let server = issuing(&carried).await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, Some(CLAIM))?;
    let Credentials::Bearer(sent) =
        credentials(&lane, &organisation_conveyance(true, false)?).await?
    else {
        return Err("a bearer token".into());
    };
    assert_eq!(carried, sent.expose_secret());

    let server = issuing(&issued(false)?).await;
    let lane = lane_of(&server, UPSTREAM, Organisations::ReadOnly, Some(CLAIM))?;
    let (refused, account) = refuses(&lane, &organisation_conveyance(true, false)?, |error| {
        matches!(
            error,
            LaneError::Onward(OnwardError::Rejected(Rejected::Correlation))
        )
    })
    .await?;
    assert!(refused, "{account}");
    Ok(())
}
