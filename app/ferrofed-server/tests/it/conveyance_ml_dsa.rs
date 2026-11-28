// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ML-DSA key of `[signing]` (RFC 9964): published in the JWK Set beside
//! the EC keys, and signing the caller token of each node whose credentials
//! section sets `conveyance_ml_dsa`, while every other node's token is signed
//! with the current EC key (§13.1, N24, N25, CP-16). A node asking for an
//! ML-DSA token with no ML-DSA key, and an ML-DSA key file holding another
//! key, are refused at load.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use ferrofed_engine::conveyance::{HEADER, TYPE};
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::error::credentials::{CredentialsFault, SigningKeyError};
use ferrofed_server::jwks::JwkSet;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{Curve, MlDsa};
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::jws::compact;
use ferrohealth_smart::jws::jwk::Jwk;
use http::{Request, StatusCode};

use crate::facade::{
    EHR_A, EHR_B, body, crossref, gateway, node_answering, patient_query, post, registry,
};
use crate::support::{JWKS_URI, call, injected};

type TestResult = Result<(), Box<dyn Error>>;

/// A key file in `dir` under `name` holding `pem`, as a TOML string, and
/// the key's `kid`.
fn key_file(dir: &Path, name: &str, pem: &str) -> Result<(toml::Value, String), Box<dyn Error>> {
    let file = dir.join(name);
    std::fs::write(&file, pem)?;
    let kid = AssertionKey::from_pem(pem.as_bytes(), None, None)?
        .kid()
        .to_owned();
    Ok((toml::Value::String(file.display().to_string()), kid))
}

/// The JWK Set `app` publishes, read as public keys of every type the
/// family verifies.
pub(crate) async fn published(app: &Router) -> Result<JwkSet, Box<dyn Error>> {
    let request = Request::get("/.well-known/jwks.json").body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    if status != StatusCode::OK {
        return Err(format!("the JWK Set answered {status}: {text}").into());
    }
    Ok(serde_json::from_str(&text)?)
}

/// The key of `keys` with `kid`.
pub(crate) fn key<'a>(keys: &'a JwkSet, kid: &str) -> Result<&'a Jwk, Box<dyn Error>> {
    Ok(keys
        .keys
        .iter()
        .find(|key| key.kid() == Some(kid))
        .ok_or("the key is published")?)
}

/// The one conveyed token `server` received.
async fn token(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("{} requests", requests.len()).into());
    };
    Ok(request
        .headers
        .get(HEADER)
        .ok_or("the request carries the token")?
        .to_str()?
        .to_owned())
}

/// `token` verified as a node verifies it against `keys`: its type, its
/// algorithm as its key is published with it, its signature by `kid`, and
/// `aud` `audience`; returns its algorithm and `kid`.
fn verified(
    token: &str,
    keys: &JwkSet,
    audience: &str,
) -> Result<(Algorithm, String), Box<dyn Error>> {
    let decoded = compact::decode(token)?;
    let header = decoded.header();
    let kid = header.kid.clone().ok_or("the token names its key")?;
    let published = key(keys, &kid)?;
    if header.typ.as_deref() != Some(TYPE) || published.algorithm() != Some(header.alg) {
        return Err(format!("typ {:?}, alg {}", header.typ, header.alg.name()).into());
    }
    let claims = decoded.verify_claims(published)?;
    assert_eq!(
        Some(audience),
        claims.get("aud").and_then(|aud| aud.as_str()),
        "the token is for its node"
    );
    Ok((header.alg, kid))
}

/// The configuration of a development gateway over node A at `a` and node
/// B at `b`, both holding the patient, with `tables`.
fn tables(a: &str, b: &str, tables: &str) -> (String, String) {
    (
        registry(a, b, ""),
        format!(
            "{}\n{tables}",
            crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
        ),
    )
}

/// RFC 9964 §5: node A, whose section sets `conveyance_ml_dsa` and names no
/// credentials, receives a caller token signed with the ML-DSA-65 key, and
/// node B one signed with the current P-384 key; the JWK Set publishes both
/// and each token verifies against it.
// conformance: CP-16
#[tokio::test]
async fn a_node_asking_for_ml_dsa_receives_a_caller_token_signed_with_it() -> TestResult {
    let a = node_answering("8849182c-82ad-4088-a07f-48ead4180515::node-a::1").await;
    let b = node_answering("6cb19121-4307-4a29-9c1c-b6d6a2ab3b77::node-b::1").await;
    let dir = tempfile::tempdir()?;
    let (current, current_kid) = key_file(dir.path(), "current.pem", &Curve::P384.pem()?)?;
    let (ml_dsa, ml_dsa_kid) = key_file(dir.path(), "ml-dsa.pem", &MlDsa::MlDsa65.pem()?)?;
    let (registry, tables) = tables(
        &a.uri(),
        &b.uri(),
        &format!(
            "[signing]\nkey_file = {current}\nml_dsa_key_file = {ml_dsa}\njwks_uri = \"{JWKS_URI}\"\n\n[credentials.\"node-a-pub\"]\nconveyance_ml_dsa = true\n"
        ),
    );
    let app = gateway(dir.path(), &registry, "profile = \"development\"", &tables)?;
    let keys = published(&app).await?;
    let kids: Vec<Option<&str>> = keys.keys.iter().map(Jwk::kid).collect();
    assert_eq!(
        vec![Some(current_kid.as_str()), Some(ml_dsa_kid.as_str())],
        kids,
        "the current key, then the ML-DSA key"
    );

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        (Algorithm::MlDsa65, ml_dsa_kid),
        verified(&token(&a).await?, &keys, "node-a-pub")?,
        "node A's token is signed with the ML-DSA key"
    );
    assert_eq!(
        (Algorithm::Es384, current_kid),
        verified(&token(&b).await?, &keys, "node-b-pub")?,
        "node B's token is signed with the current key"
    );
    Ok(())
}

/// The error the development gateway `tables` configure is refused with.
fn refused(dir: &Path, tables: &str) -> Result<ConfigError, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("http://127.0.0.1:9/a", "http://127.0.0.1:9/b", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\n{tables}"
    );
    match Config::from_sources(Some(&injected(&text)), &BTreeMap::new())?.resolve() {
        Ok(_) => Err("the configuration was accepted".into()),
        Err(error) => Ok(error),
    }
}

/// A node asking for an ML-DSA caller token is refused, naming its section,
/// when `[signing]` names no ML-DSA key; an ML-DSA key file holding an EC
/// key is refused, naming the file's key.
#[test]
fn an_ml_dsa_conveyance_without_an_ml_dsa_key_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (current, _) = key_file(dir.path(), "current.pem", &Curve::P384.pem()?)?;
    let signing = format!("[signing]\nkey_file = {current}\njwks_uri = \"{JWKS_URI}\"\n");
    let error = refused(
        dir.path(),
        &format!("{signing}\n[credentials.\"node-a-pub\"]\nconveyance_ml_dsa = true\n"),
    )?;
    assert!(
        matches!(
            &error,
            ConfigError::Credentials(CredentialsFault::ConveyanceMlDsa { section })
                if section == "credentials.node-a-pub"
        ),
        "{error:?}"
    );
    let error = refused(
        dir.path(),
        &format!("{signing}ml_dsa_key_file = {current}\n"),
    )?;
    assert!(
        matches!(
            &error,
            ConfigError::Credentials(CredentialsFault::SigningKey {
                key,
                source: SigningKeyError::NotMlDsa,
            }) if key == "signing.ml_dsa_key_file"
        ),
        "{error:?}"
    );
    Ok(())
}
