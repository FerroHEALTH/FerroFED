// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check authenticates to a node as the federated query does:
//! with the token its OAuth 2.0 grant obtained (§12b.1, §13.1, N25).

use std::sync::Mutex;

use ferrofed_server::admission::report::{Condition, Verdict};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use ferrofed_testkit::unreachable;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{crossref, registry};

use super::{
    Issued, Minting, ReadableSubject, Reading, SYSTEM_A, TestResult, V4, check_a, federation,
    verdict,
};

/// The client node A's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// A node that creates and reads EHRs for a request carrying a token
/// `endpoint` issued, and answers `401` to every other.
async fn node_requiring(endpoint: &TokenEndpoint) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .and(ReadableSubject)
        .and(endpoint.bearer())
        .respond_with(Minting {
            ids: Mutex::new(V4.iter().map(|id| (*id).to_owned()).collect()),
            issued: Issued::default(),
        })
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/ehr/[^/]+$"))
        .and(endpoint.bearer())
        .respond_with(Reading {
            system_id: SYSTEM_A.to_owned(),
        })
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

// conformance: CP-17
#[tokio::test]
async fn the_admission_check_authenticates_with_the_onward_grant() -> TestResult {
    admitted_with("").await
}

/// Beside an onward lane, which carries every caller's request, the
/// admission check, which no caller originates, is still sent under the
/// token of the endpoint's client credentials grant (§13.1, N25).
// conformance: CP-17
#[tokio::test]
async fn beside_an_onward_lane_the_admission_check_uses_the_client_credentials_grant() -> TestResult
{
    admitted_with(concat!(
        "\n[credentials.\"node-a-pub\".credentials.onward]\n",
        "issuer = \"https://federation.cdr-a.example.org\"\n",
        "kind = \"persons\"\n",
        "profile = \"rfc9068\"\n",
        "audiences = [\"https://cdr-a.example.org/openehr\"]\n",
        "indicators = [\"https://cdr-a.example.org/openehr\"]\n\n",
        "[credentials.\"node-a-pub\".credentials.onward.keys]\n",
        "max_onward_lifetime = '1h'\n",
    ))
    .await
}

/// Runs the admission check over node A, which admits only a token of the
/// endpoint's client credentials grant, with `onward` beside that grant.
async fn admitted_with(onward: &str) -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    let a = node_requiring(&endpoint).await;
    let dir = tempfile::tempdir()?;
    let key = dir.path().join("key.pem");
    std::fs::write(&key, oauth::Curve::P384.pem()?)?;
    let key = toml::Value::String(key.display().to_string());
    let tables = format!(
        "{}\n[signing]\nkey_file = {key}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n\n[credentials.\"node-a-pub\".credentials.oauth]\nclient_auth = \"private_key_jwt\"\nprivate_key_file = {key}\ntoken_endpoint = \"{}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"system/composition-*.cr\"\n",
        crossref(&[("node-a", V4[0])]),
        endpoint.token_url()
    ) + onward;
    let federation = federation(
        dir.path(),
        &registry(&a.uri(), unreachable::BASE, ""),
        "profile = \"development\"",
        &tables,
    )?;
    let signing = federation.signing().ok_or("the keys are configured")?;
    let mut published: jsonwebtoken::jwk::JwkSet =
        serde_json::from_value(signing.keys.published())?;
    published
        .keys
        .extend(federation.client_keys().iter().cloned());
    endpoint.trust(published);

    let report = check_a(&federation).await?;
    assert_eq!(
        Verdict::Pass,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    assert!(endpoint.issuance().issued() >= 1, "a token was obtained");
    Ok(())
}
