// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ML-DSA on the gateway's own grant (RFC 9964), through the real
//! configuration path: an `oauth` grant whose `private_key_file` holds an
//! ML-DSA-65 key authenticates with an ML-DSA-65 client assertion, whose
//! key the JWK Set publishes (RFC 7523 §2.2), and a grant whose
//! `dpop_key_file` holds one proves every request to its node and token
//! endpoint with ML-DSA-65 (RFC 9449 §4.2; §13.1, N25, CP-17).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;

use axum::Router;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, Curve, MlDsa, TokenEndpoint};
use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::jws::compact;
use http::StatusCode;
use wiremock::matchers::{header, method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use crate::conveyance_ml_dsa::{key, published};
use crate::facade::{
    EHR_A, body, crossref, gateway, node_answering, patient_query, post, registry,
};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The client node A's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The access token the plain token endpoint issues.
const ISSUED: &str = "synthetic-ml-dsa-access-token";

/// A one-row answer from a node.
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// `pem` written to `dir/name`, as a TOML string of its path.
fn key_file(dir: &Path, name: &str, pem: &str) -> Result<toml::Value, Box<dyn Error>> {
    let path = dir.join(name);
    std::fs::write(&path, pem)?;
    Ok(toml::Value::String(path.display().to_string()))
}

/// A node A answering the query to a request `matcher` admits, and `401` to
/// every other.
async fn node(matcher: impl Match + 'static) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(matcher)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(ONE_ROW.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// The development gateway over node A at `a` and an answering node B, the
/// patient at node A, node A reached under node A's `private_key_jwt` grant
/// at `token_url` signed by `client_key` as `alg`, with `extra` keys.
async fn served(
    dir: &Path,
    a: &str,
    token_url: &str,
    (client_key, alg): (&toml::Value, &str),
    extra: &str,
) -> Result<(Router, Server), Box<dyn Error>> {
    let b = node_answering("6cb19121-4307-4a29-9c1c-b6d6a2ab3b77::node-b::1").await;
    let tables = format!(
        "{}\n[credentials.\"node-a-pub\".credentials.oauth]\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"system/aql-*.s\"\nprivate_key_file = {client_key}\nassertion_alg = \"{alg}\"\n{extra}",
        crossref(&[("node-a", EHR_A)])
    );
    let app = gateway(
        dir,
        &registry(a, &b.uri(), ""),
        "profile = \"development\"",
        &tables,
    )?;
    Ok((app, b))
}

/// The value of `name` in the form body `form`.
fn field(form: &[u8], name: &str) -> Option<String> {
    url::form_urlencoded::parse(form)
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// RFC 7523 §2.2, RFC 9964 §5: a grant whose key is an ML-DSA-65 key
/// authenticates with an ML-DSA-65 assertion of its `client_id` for the
/// token endpoint, which verifies against the key the JWK Set publishes,
/// and node A receives the token issued.
// conformance: CP-17
#[tokio::test]
async fn an_ml_dsa_client_key_signs_its_assertion_and_is_published() -> TestResult {
    let token_server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(oauth::TOKEN_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": ISSUED,
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(&token_server)
        .await;
    let token_url = format!("{}{}", token_server.uri(), oauth::TOKEN_PATH);
    let a = node(header("authorization", format!("Bearer {ISSUED}").as_str())).await;
    let dir = tempfile::tempdir()?;
    let client_key = key_file(dir.path(), "client.pem", &MlDsa::MlDsa65.pem()?)?;
    let (app, _b) = served(
        dir.path(),
        &a.uri(),
        &token_url,
        (&client_key, "ML-DSA-65"),
        "",
    )
    .await?;
    let keys = published(&app).await?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let requests = token_server
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("{} token requests", requests.len()).into());
    };
    let assertion = field(&request.body, "client_assertion").ok_or("an assertion is sent")?;
    let decoded = compact::decode(&assertion)?;
    assert_eq!(Algorithm::MlDsa65, decoded.header().alg);
    let kid = decoded
        .header()
        .kid
        .clone()
        .ok_or("the assertion names its key")?;
    let jwk = key(&keys, &kid)?;
    assert_eq!("AKP", jwk.key_type());
    let claims = decoded.verify_claims(jwk)?;
    for (claim, expected) in [("iss", CLIENT_ID), ("sub", CLIENT_ID), ("aud", &token_url)] {
        assert_eq!(
            Some(expected),
            claims.get(claim).and_then(|value| value.as_str()),
            "{claim}"
        );
    }
    Ok(())
}

/// RFC 9449 §4.2, §7.1, RFC 9964 §5: a grant whose `DPoP` key is an
/// ML-DSA-65 key proves its token request and every request to node A with
/// ML-DSA-65, and node A receives the bound token under the `DPoP` scheme.
// conformance: CP-17
#[tokio::test]
async fn an_ml_dsa_dpop_key_proves_every_request_to_its_node() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.require_dpop();
    let a = node(endpoint.dpop_bound(None)).await;
    let dir = tempfile::tempdir()?;
    let client_key = key_file(dir.path(), "client.pem", &Curve::P384.pem()?)?;
    let dpop_key = key_file(dir.path(), "dpop.pem", &MlDsa::MlDsa65.pem()?)?;
    let (app, _b) = served(
        dir.path(),
        &a.uri(),
        &endpoint.token_url(),
        (&client_key, "ES384"),
        &format!("dpop_key_file = {dpop_key}\n"),
    )
    .await?;
    endpoint.trust(crate::conveyance::published(&app).await?);

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, endpoint.issuance().issued());
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("{} requests at node A", requests.len()).into());
    };
    let proof = request
        .headers
        .get(ferrofed_testkit::dpop::HEADER)
        .ok_or("the request carries a proof")?
        .to_str()?;
    assert_eq!(Algorithm::MlDsa65, compact::decode(proof)?.header().alg);
    Ok(())
}
