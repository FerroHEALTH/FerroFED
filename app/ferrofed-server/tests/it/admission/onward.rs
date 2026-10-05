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
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    let a = node_requiring(&endpoint).await;
    let dir = tempfile::tempdir()?;
    let key = dir.path().join("key.pem");
    std::fs::write(&key, oauth::es384_pem()?)?;
    let key = toml::Value::String(key.display().to_string());
    let tables = format!(
        "{}\n[signing]\nkey_file = {key}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n\n[credentials.\"node-a-pub\".oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"{}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"system/composition-*.cr\"\n",
        crossref(&[("node-a", V4[0])]),
        endpoint.token_url()
    );
    let federation = federation(
        dir.path(),
        &registry(&a.uri(), unreachable::BASE, ""),
        "profile = \"development\"",
        &tables,
    )?;
    let signing = federation.signing().ok_or("the keys are configured")?;
    endpoint.trust(signing.keys.published());

    let report = check_a(&federation).await?;
    assert_eq!(
        Verdict::Pass,
        verdict(&report, Condition::EhrIdGeneration)?,
        "{report}"
    );
    assert!(endpoint.issued() >= 1, "a token was obtained");
    Ok(())
}
