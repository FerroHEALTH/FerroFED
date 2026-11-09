// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The performance gate's token endpoint (#1712): the gateway's own token,
//! a caller's token exchanged for one of the onward issuer, the client
//! authenticated by a `private_key_jwt` assertion, every refusal an RFC 6749
//! §5.2 error, and each call counted.
#![expect(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};

use base64::Engine as _;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::oauth::{Curve, JWT_BEARER};
use ferrofed_testkit::perf_token::{Calls, ErrorAnswer, PerfTokenEndpoint, Settings, TokenAnswer};
use ferrohealth_smart::client::assertion::AssertionKey;
use serde::Deserialize;

type TestResult = Result<(), Box<dyn Error>>;

const PERSONS: &str = "https://perf-onward.ferrofed.invalid";
const MACHINES: &str = "https://perf-onward.ferrofed.invalid/machines";
const CALLER: &str = "https://quickstart-issuer.ferrofed.invalid";
const CLIENT: &str = "ferrofed-perf";
const NODE: &str = "ferroehr-a";
const EXCHANGE: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange";

/// The claims of an issued token the tests read.
#[derive(Debug, Deserialize)]
struct Issued {
    iss: String,
    sub: String,
    client_id: String,
    aud: String,
    scope: String,
    exp: i64,
    iat: i64,
}

/// A key set, its keys counted.
#[derive(Debug, Deserialize)]
struct KeySet {
    keys: Vec<serde::de::IgnoredAny>,
}

/// The device over a fresh caller issuer, with the temporary directory its
/// key sets are written to.
async fn device() -> Result<(PerfTokenEndpoint, Issuer, tempfile::TempDir), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let caller = Issuer::new(CALLER)?;
    caller.write_jwks(&dir.path().join("caller.json"))?;
    let endpoint = PerfTokenEndpoint::start(Settings {
        listen: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        persons_issuer: PERSONS.to_owned(),
        persons_jwks: dir.path().join("persons.json"),
        machines_issuer: MACHINES.to_owned(),
        machines_jwks: dir.path().join("machines.json"),
        audience: NODE.to_owned(),
        client_id: CLIENT.to_owned(),
        client_key: dir.path().join("client.pem"),
        caller_issuer: CALLER.to_owned(),
        caller_jwks: dir.path().join("caller.json"),
        lifetime_s: 3600,
    })
    .await?;
    Ok((endpoint, caller, dir))
}

/// A client assertion for `endpoint` signed with the PKCS#8 PEM `pem` by
/// the signer the gateway's `private_key_jwt` grant uses.
fn assertion(endpoint: &PerfTokenEndpoint, pem: &str) -> Result<String, Box<dyn Error>> {
    let key = AssertionKey::from_pem(pem.as_bytes(), None, None)?;
    Ok(key.assertion(CLIENT, &endpoint.token_url())?)
}

/// Posts `form` to the token endpoint, authenticated by an assertion signed
/// with the PEM key `pem`, and returns the status and the body.
async fn post_as(
    endpoint: &PerfTokenEndpoint,
    pem: &str,
    form: &str,
) -> Result<(u16, String), Box<dyn Error>> {
    let assertion = assertion(endpoint, pem)?;
    let assertion_type: String =
        url::form_urlencoded::byte_serialize(JWT_BEARER.as_bytes()).collect();
    let answer = reqwest::Client::new()
        .post(endpoint.token_url())
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!(
            "{form}&client_id={CLIENT}&client_assertion_type={assertion_type}&client_assertion={assertion}"
        ))
        .send()
        .await?;
    let status = answer.status().as_u16();
    Ok((status, answer.text().await?))
}

/// Posts `form` as the client, with the key the device wrote to `dir`.
async fn post(
    endpoint: &PerfTokenEndpoint,
    dir: &tempfile::TempDir,
    form: &str,
) -> Result<(u16, String), Box<dyn Error>> {
    let pem = std::fs::read_to_string(dir.path().join("client.pem"))?;
    post_as(endpoint, &pem, form).await
}

/// The error code of a refusal `body`.
fn error(body: &str) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::from_str::<ErrorAnswer>(body)?.error)
}

/// The claims of the compact JWS `token`, read without verifying it.
fn claims(token: &str) -> Result<Issued, Box<dyn Error>> {
    let payload = token.split('.').nth(1).ok_or("the token has no payload")?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload)?;
    Ok(serde_json::from_slice(&bytes)?)
}

#[tokio::test]
async fn the_gateway_s_own_token_comes_from_the_machines_issuer() -> TestResult {
    let (endpoint, _caller, dir) = device().await?;
    let (status, body) = post(
        &endpoint,
        &dir,
        "grant_type=client_credentials&scope=system%2Faql-*.s",
    )
    .await?;
    assert_eq!(status, 200, "{body}");
    let answer: TokenAnswer = serde_json::from_str(&body)?;
    assert_eq!((answer.expires_in, answer.issued_token_type), (3600, None));
    let issued = claims(&answer.access_token)?;
    assert_eq!(issued.iss, MACHINES);
    assert_eq!(issued.sub, CLIENT);
    assert_eq!(issued.client_id, CLIENT);
    assert_eq!(issued.aud, NODE);
    assert_eq!(issued.scope, "system/aql-*.s");
    assert_eq!(issued.exp - issued.iat, 3600);
    let machines: KeySet =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("machines.json"))?)?;
    assert_eq!(machines.keys.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_caller_s_token_is_exchanged_for_one_of_the_onward_issuer() -> TestResult {
    let (endpoint, caller, dir) = device().await?;
    let mut subject = Claims::new(CALLER, "ferrofed-quickstart");
    "quickstart-user".clone_into(&mut subject.sub);
    let subject = caller.mint(&subject)?;
    let (status, body) = post(
        &endpoint,
        &dir,
        &format!("{EXCHANGE}&subject_token={subject}&subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token&resource=node-a&scope=user%2Faql-*.s"),
    )
    .await?;
    assert_eq!(status, 200, "{body}");
    let answer: TokenAnswer = serde_json::from_str(&body)?;
    assert_eq!(
        answer.issued_token_type.as_deref(),
        Some("urn:ietf:params:oauth:token-type:access_token")
    );
    let issued = claims(&answer.access_token)?;
    assert_eq!(issued.iss, PERSONS);
    assert_eq!(issued.sub, "quickstart-user");
    assert_eq!(issued.client_id, CLIENT);
    assert_eq!(issued.aud, "node-a");
    assert_eq!(issued.scope, "user/aql-*.s");
    assert_eq!(
        endpoint.calls(),
        Calls {
            client_credentials: 0,
            token_exchange: 1,
            refused: 0,
        }
    );
    Ok(())
}

#[tokio::test]
async fn an_exchange_naming_no_scope_is_granted_the_subject_s() -> TestResult {
    let (endpoint, caller, dir) = device().await?;
    let mut subject = Claims::new(CALLER, "ferrofed-quickstart");
    subject.scope = Some(String::from("user/aql-*.s user/template-*.cruds"));
    let subject = caller.mint(&subject)?;
    let (status, body) = post(
        &endpoint,
        &dir,
        &format!("{EXCHANGE}&subject_token={subject}"),
    )
    .await?;
    assert_eq!(status, 200, "{body}");
    let issued = claims(&serde_json::from_str::<TokenAnswer>(&body)?.access_token)?;
    assert_eq!(issued.scope, "user/aql-*.s user/template-*.cruds");
    assert_eq!(issued.aud, NODE);
    Ok(())
}

#[tokio::test]
async fn every_refusal_is_an_error_answer_and_counted() -> TestResult {
    let (endpoint, _caller, dir) = device().await?;
    let (status, body) = post_as(
        &endpoint,
        &Curve::P256.pem()?,
        "grant_type=client_credentials",
    )
    .await?;
    assert_eq!((status, error(&body)?.as_str()), (400, "invalid_client"));
    let secret = reqwest::Client::new()
        .post(endpoint.token_url())
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!(
            "grant_type=client_credentials&client_id={CLIENT}&client_secret=synthetic"
        ))
        .send()
        .await?;
    assert_eq!(secret.status().as_u16(), 400);
    assert_eq!(error(&secret.text().await?)?, "invalid_client");
    let stranger = Issuer::new("https://stranger.example")?;
    let subject = stranger.mint(&Claims::new("https://stranger.example", "x"))?;
    let (status, body) = post(
        &endpoint,
        &dir,
        &format!("{EXCHANGE}&subject_token={subject}"),
    )
    .await?;
    assert_eq!((status, error(&body)?.as_str()), (400, "invalid_grant"));
    let (status, body) = post(&endpoint, &dir, "grant_type=password").await?;
    assert_eq!(
        (status, error(&body)?.as_str()),
        (400, "unsupported_grant_type")
    );
    let counted: Calls = serde_json::from_str(
        &reqwest::get(format!("http://{}/calls", endpoint.address()))
            .await?
            .text()
            .await?,
    )?;
    assert_eq!(
        counted,
        Calls {
            client_credentials: 0,
            token_exchange: 0,
            refused: 4,
        }
    );
    Ok(())
}
