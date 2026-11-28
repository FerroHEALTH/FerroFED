// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ML-DSA at the gate (RFC 9964): an issuer whose `algorithms` list an
//! ML-DSA parameter set has its ML-DSA access tokens admitted, and
//! `[auth.dpop] algorithms` admits a proof signed with one; at their
//! default lists, both are refused. The lists are written as configuration
//! and resolved, so the test reads them where an operator sets them.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthSettings;
use ferrofed_testkit::oauth::MlDsa;
use ferrohealth_smart::client::dpop::{DpopKey, Role};
use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::jws::compact::{self, Header};
use ferrohealth_smart::jws::key::{PrivateKey, SigningKey};
use http::{Method, StatusCode, header};
use serde_json::json;
use url::Url;

use super::dpop::EXTERNAL;
use super::{Gateway, TestResult, claims, query, sent};
use crate::support::caller::{AUDIENCE, ISSUER};
use crate::support::toml_path;

/// The `kid` of the ML-DSA issuer's key.
const KID: &str = "ml-dsa-issuer-1";

/// An issuer signing its tokens with a fresh ML-DSA-65 key, and the file
/// its JWK Set is written to.
struct MlDsaIssuer {
    key: SigningKey,
    _dir: tempfile::TempDir,
    jwks: std::path::PathBuf,
}

impl MlDsaIssuer {
    /// A fresh issuer, its JWK Set written to a file of its own.
    fn new() -> Result<Self, Box<dyn Error>> {
        let private = PrivateKey::from_pem(MlDsa::MlDsa65.pem()?.as_bytes())?;
        let key = SigningKey::new(private, Algorithm::MlDsa65)?;
        let dir = tempfile::tempdir()?;
        let jwks = dir.path().join("jwks.json");
        let jwk = key
            .public_jwk()
            .clone()
            .with_kid(KID)
            .with_signature_use()
            .to_value();
        std::fs::write(&jwks, json!({ "keys": [jwk] }).to_string())?;
        Ok(Self {
            key,
            _dir: dir,
            jwks,
        })
    }

    /// A token over the suite's default claims, bound to `jkt` when given.
    fn mint(&self, jkt: Option<&str>) -> Result<String, Box<dyn Error>> {
        let mut minted = claims();
        minted.cnf = jkt.map(|jkt| json!({ "jkt": jkt }));
        let mut header = Header::new(Algorithm::MlDsa65);
        header.kid = Some(KID.to_owned());
        header.typ = Some(String::from("at+jwt"));
        Ok(compact::encode(&header, &minted, &self.key)?)
    }
}

/// The `[auth]` the configuration resolves: the suite's issuer verified
/// against `jwks`, with `issuer_keys` in its table and `dpop_keys` in
/// `[auth.dpop]`.
fn resolved(
    jwks: &Path,
    issuer_keys: &str,
    dpop_keys: &str,
) -> Result<AuthSettings, Box<dyn Error>> {
    let text = format!(
        "[gateway]\npublic_url = \"{EXTERNAL}\"\n\n[auth]\n\n[auth.issuer.ml]\nissuer = \"{ISSUER}\"\naudience = [\"{AUDIENCE}\"]\njwks_file = {}\n{issuer_keys}\n[auth.issuer.ml.attributes]\nmapping = \"iua\"\nauthorization_details_purposes = true\n\n[auth.dpop]\n{dpop_keys}\n\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\ndemographic_clients = [\"{}\"]\n",
        toml_path(jwks),
        claims().client_id
    );
    Ok(Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()?
        .server
        .auth)
}

/// The suite's patient query, bearing `token`.
fn bearing(token: &str) -> Result<http::Request<axum::body::Body>, Box<dyn Error>> {
    super::bearing(query()?, token)
}

/// The suite's patient query, carrying `token` in the `DPoP` scheme and a
/// proof of it by `key`.
fn proven(key: &DpopKey, token: &str) -> Result<http::Request<axum::body::Body>, Box<dyn Error>> {
    let mut request = query()?;
    let url = Url::parse(&format!("{EXTERNAL}{}", request.uri().path()))?;
    let proof = key.proof(&Method::POST, &url, Role::Resource, Some(token))?;
    let headers = request.headers_mut();
    headers.insert(header::AUTHORIZATION, format!("DPoP {token}").parse()?);
    headers.insert("dpop", proof.parse()?);
    Ok(request)
}

/// RFC 9964 §5: an issuer whose `algorithms` list ML-DSA-65 has its
/// ML-DSA-65 tokens admitted.
// conformance: CP-17
#[tokio::test]
async fn an_ml_dsa_token_is_admitted_by_an_issuer_that_lists_ml_dsa() -> TestResult {
    let issuer = MlDsaIssuer::new()?;
    let auth = resolved(&issuer.jwks, "algorithms = [\"ML-DSA-65\"]\n", "")?;
    let gateway = Gateway::with(auth).await?;
    let (status, _, text) = sent(&gateway.app, bearing(&issuer.mint(None)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

/// RFC 8725 §3.1: an issuer at its default `algorithms`, which list no
/// ML-DSA parameter set, refuses an ML-DSA token, and no node is asked.
#[tokio::test]
async fn an_ml_dsa_token_is_refused_by_an_issuer_at_its_default_algorithms() -> TestResult {
    let issuer = MlDsaIssuer::new()?;
    let auth = resolved(&issuer.jwks, "", "")?;
    let gateway = Gateway::with(auth).await?;
    let (status, _, text) = sent(&gateway.app, bearing(&issuer.mint(None)?)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    gateway.nobody_asked().await
}

/// RFC 9449 §4.3, RFC 9964 §5: `[auth.dpop] algorithms` listing ML-DSA-65
/// admits a token bound to an ML-DSA-65 key, proven by that key.
// conformance: CP-17
#[tokio::test]
async fn an_ml_dsa_proof_is_admitted_when_auth_dpop_lists_ml_dsa() -> TestResult {
    let issuer = MlDsaIssuer::new()?;
    let auth = resolved(
        &issuer.jwks,
        "algorithms = [\"ML-DSA-65\"]\n",
        "algorithms = [\"ML-DSA-65\"]\n",
    )?;
    let gateway = Gateway::with(auth).await?;
    let key = DpopKey::from_pem(MlDsa::MlDsa65.pem()?.as_bytes())?;
    let token = issuer.mint(Some(key.thumbprint()))?;
    let (status, _, text) = sent(&gateway.app, proven(&key, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

/// RFC 9449 §4.3: `[auth.dpop]` at its default `algorithms`, which list no
/// ML-DSA parameter set, refuses a proof signed with one, and no node is
/// asked.
#[tokio::test]
async fn an_ml_dsa_proof_is_refused_at_the_default_dpop_algorithms() -> TestResult {
    let issuer = MlDsaIssuer::new()?;
    let auth = resolved(&issuer.jwks, "algorithms = [\"ML-DSA-65\"]\n", "")?;
    let gateway = Gateway::with(auth).await?;
    let key = DpopKey::from_pem(MlDsa::MlDsa65.pem()?.as_bytes())?;
    let token = issuer.mint(Some(key.thumbprint()))?;
    let (status, _, text) = sent(&gateway.app, proven(&key, &token)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    gateway.nobody_asked().await
}
