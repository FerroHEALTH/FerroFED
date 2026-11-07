// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `DPoP` request at the gate (RFC 9449): with `[auth.dpop]`, a token bound
//! to the key of the request's proof is admitted, and a proof sent a second
//! time is refused (§11.1), here in the memory of one process, which a
//! gateway with no store of its own records proofs in; without the table no
//! `DPoP` request is admitted. The proofs of several replicas over one store
//! are `e2e::dpop_replay`'s.

use ferrofed_server::config::auth::AuthSettings;
use ferrofed_testkit::dpop::Holder;
use ferrohealth_smart::sender::proof::ProofSettings;
use http::StatusCode;
use serde_json::json;
use url::Url;

use super::{Gateway, TestResult, claims, minted, query, sent};

/// The base URL the test clients address the gateway at.
pub(crate) const EXTERNAL: &str = "https://gateway.example.test";

/// The `[auth]` of every test gateway, checking `DPoP` proofs addressed to
/// [`EXTERNAL`].
pub(crate) fn proving() -> Result<AuthSettings, url::ParseError> {
    Ok(AuthSettings {
        dpop: Some(ProofSettings::new(Url::parse(EXTERNAL)?)),
        ..crate::support::caller::auth()
    })
}

/// `request` as `holder` sends it: a token bound to its key in the `DPoP`
/// scheme, and `proof`, or a fresh proof of the request's method and path
/// at [`EXTERNAL`] when none is given; and the proof it carries.
pub(crate) fn bound(
    holder: &Holder,
    mut request: http::Request<axum::body::Body>,
    proof: Option<&str>,
) -> Result<(http::Request<axum::body::Body>, String), Box<dyn std::error::Error>> {
    let mut bound = claims();
    bound.cnf = Some(json!({ "jkt": holder.jkt() }));
    let token = minted(&bound)?;
    let proof = match proof {
        Some(proof) => proof.to_owned(),
        None => holder.prove(
            request.method().as_str(),
            &format!("{EXTERNAL}{}", request.uri().path()),
            &token,
        )?,
    };
    let headers = request.headers_mut();
    headers.insert(
        http::header::AUTHORIZATION,
        format!("DPoP {token}").parse()?,
    );
    headers.insert("dpop", proof.parse()?);
    Ok((request, proof))
}

/// The suite's patient query as `holder` sends it ([`bound`]).
fn bound_query(
    holder: &Holder,
    proof: Option<&str>,
) -> Result<(http::Request<axum::body::Body>, String), Box<dyn std::error::Error>> {
    bound(holder, query()?, proof)
}

/// RFC 9449 §7.1: a token bound to the key of the request's proof is
/// admitted in the `DPoP` scheme.
#[tokio::test]
async fn a_token_bound_to_the_proofs_key_is_admitted() -> TestResult {
    let gateway = Gateway::with(proving()?).await?;
    let holder = Holder::generate()?;
    let (request, _) = bound_query(&holder, None)?;
    let (status, _, text) = sent(&gateway.app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

/// RFC 9449 §11.1: a proof sent a second time is refused as a proof that
/// does not hold, and a fresh proof of the same key is admitted.
#[tokio::test]
async fn a_replayed_proof_is_refused() -> TestResult {
    let gateway = Gateway::with(proving()?).await?;
    let holder = Holder::generate()?;
    let (first, proof) = bound_query(&holder, None)?;
    let (status, _, text) = sent(&gateway.app, first).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (again, _) = bound_query(&holder, Some(&proof))?;
    let (status, headers, text) = sent(&gateway.app, again).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    let challenge = headers
        .get(http::header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(challenge.starts_with("DPoP"), "RFC 9449 §7.2: {challenge}");
    let (fresh, _) = bound_query(&holder, None)?;
    let (status, _, text) = sent(&gateway.app, fresh).await?;
    assert_eq!(StatusCode::OK, status, "a fresh proof: {text}");
    Ok(())
}

/// RFC 9449 §7.1: without `[auth.dpop]` the gateway admits no `DPoP`
/// request, and no node is asked.
#[tokio::test]
async fn without_the_table_no_dpop_request_is_admitted() -> TestResult {
    let gateway = Gateway::with(crate::support::caller::auth()).await?;
    let holder = Holder::generate()?;
    let (request, _) = bound_query(&holder, None)?;
    let (status, _, text) = sent(&gateway.app, request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    gateway.nobody_asked().await
}

/// `[auth.dpop]` as written: its `external_url` is the base a proof's `htu`
/// is compared with (RFC 9449 §4.3, check 9), read through the
/// configuration as the gateway reads it.
#[test]
fn the_table_is_read_from_the_configuration() -> TestResult {
    let text = format!(
        "{}\n[auth.dpop]\nexternal_url = \"{EXTERNAL}\"\nmax_age_seconds = 30\n",
        crate::support::caller::auth_toml()
    );
    let settings = ferrofed_server::config::Config::from_sources(
        Some(&text),
        &std::collections::BTreeMap::new(),
    )?
    .resolve()?;
    let dpop = settings.server.auth.dpop.ok_or("the table is read")?;
    assert_eq!(Url::parse(EXTERNAL)?, dpop.external_url);
    assert_eq!(std::time::Duration::from_secs(30), dpop.max_age);
    Ok(())
}
