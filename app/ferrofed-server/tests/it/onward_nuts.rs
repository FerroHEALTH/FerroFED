// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant through the real configuration path: node A, whose
//! `[credentials]` name a Nuts grant, receives a token the harness Nuts node
//! issued for a Verifiable Presentation of the gateway's credentials,
//! `DPoP`-bound to the grant's key; a refused grant fails node A with
//! nothing sent to it and no log, span or answer carrying a credential, the
//! presentation or a key; and the configuration refuses a grant it cannot
//! use (§13.1, §13.3, N25, CP-17; Annex B §B.4; Nuts RFC021; RFC 9449).
//! Every identifier, key and credential is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::transport;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, traced};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::nuts::{self, NutsNode};
use ferrofed_testkit::oauth;
use ferrofed_testkit::unreachable;
use http::{Request, StatusCode, header};
use jsonwebtoken::Algorithm;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use crate::facade::{body, crossref, patient_query, post, registry, settings_with_room};
use crate::support::{Logs, bearer_as, call, signed};

type TestResult = Result<(), Box<dyn Error>>;

const HOLDER: &str = "did:web:gateway.example.org";
const HOLDER_KID: &str = "did:web:gateway.example.org#key-1";
const ISSUER: &str = "did:web:issuer.example.org";
const ISSUER_KID: &str = "did:web:issuer.example.org#key-1";
const SCOPE: &str = "openehr-query";
const DEFINITION: &str =
    r#"{"id": "pd_synthetic", "input_descriptors": [{"id": "organization_credential"}]}"#;
const CALLER: &str = "synthetic-clinician-nuts";
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// The files a Nuts grant reads, and the credential and keys they hold.
struct Material {
    tables: String,
    credential: String,
    holder_pem: String,
}

/// A harness Nuts node trusting a fresh holder and issuer, and the
/// `[credentials]` table of node A's grant at it, its files under `dir`.
async fn material(dir: &Path) -> Result<(NutsNode, Material), Box<dyn Error>> {
    let authority = NutsNode::start("hospital-a", SCOPE, Some(300)).await;
    authority.define(DEFINITION);
    let holder_pem = oauth::p256_pem()?;
    let issuer_pem = oauth::p256_pem()?;
    authority.trust_holder(
        HOLDER,
        HOLDER_KID,
        nuts::public_jwk(&holder_pem, Algorithm::ES256)?,
    );
    authority.trust_issuer(
        ISSUER,
        ISSUER_KID,
        nuts::public_jwk(&issuer_pem, Algorithm::ES256)?,
    );
    let credential = nuts::credential(
        (&issuer_pem, ISSUER_KID),
        ISSUER,
        HOLDER,
        (
            "SyntheticOrganizationCredential",
            "Synthetic Care Organisation",
        ),
        jiff::Timestamp::now().as_second() + 3600,
    )?;
    let file = |name: &str, contents: &str| -> Result<toml::Value, Box<dyn Error>> {
        let file = dir.join(name);
        std::fs::write(&file, contents)?;
        Ok(toml::Value::String(file.display().to_string()))
    };
    let key = file("holder.pem", &holder_pem)?;
    let dpop = file("dpop.pem", &oauth::p256_pem()?)?;
    let held = file("organization.jwt", &credential)?;
    let tables = format!(
        "[credentials.\"node-a-pub\".nuts]\nauthorization_server = \"{}\"\nscope = \"{SCOPE}\"\ndid = \"{HOLDER}\"\nkid = \"{HOLDER_KID}\"\nkey_file = {key}\ndpop_key_file = {dpop}\n\n[[credentials.\"node-a-pub\".nuts.credential]]\ninput_descriptor = \"organization_credential\"\nfile = {held}\n",
        authority.issuer()
    );
    Ok((
        authority,
        Material {
            tables,
            credential,
            holder_pem,
        },
    ))
}

/// The gateway over node A at `a` and an unreachable node B, the patient
/// resolving at node A, under `profile`, with `tables`.
fn gateway(dir: &Path, a: &str, tables: &str) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, unreachable::BASE, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n{}\n{tables}",
        crossref(&[("node-a", "2222aaaa-2222-4222-8222-222222222222")])
    );
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok((
        ferrofed_server::router(Arc::clone(&state), &settings_with_room()),
        state,
    ))
}

/// A node answering the query to a request `matcher` admits, and `401` to
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

/// The patient query, sent as the test caller.
fn patient_post() -> Result<Request<axum::body::Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, bearer_as(CALLER)?.parse()?);
    Ok(request)
}

/// Node A receives the token the Nuts grant obtained, under the `DPoP`
/// scheme with a proof of the grant's key (Annex B §B.4; RFC 9449 §7.1).
// conformance: CP-17
#[tokio::test]
async fn node_a_receives_the_dpop_bound_nuts_token() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (authority, material) = material(dir.path()).await?;
    let a = node(authority.dpop_bound()).await;
    let (app, _state) = gateway(dir.path(), &a.uri(), &material.tables)?;

    let (status, text) = call(app, patient_post()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, authority.issued());
    Ok(())
}

/// A refused grant fails node A with nothing sent to it, and no log line,
/// metric or answer carries the credential, the presentation or the holder
/// key (§11; Nuts RFC021 §4.5).
// conformance: CP-17
#[tokio::test]
async fn a_refused_nuts_grant_fails_node_a_and_names_no_credential() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (authority, material) = material(dir.path()).await?;
    authority.refuse(400, "invalid_request", "the presentation was refused");
    let a = node(authority.dpop_bound()).await;
    let (app, state) = gateway(dir.path(), &a.uri(), &material.tables)?;

    let logs = Logs::default();
    let subscriber = traced(Rendering::Json, "trace", false, logs.clone(), None)?;
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, text) = call(app, patient_post()?).await?;
    drop(guard);
    assert_ne!(StatusCode::OK, status, "{text}");
    assert!(
        a.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "node A was sent nothing"
    );
    let assertions: Vec<String> = authority
        .forms()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name == "assertion")
        .map(|(_, value)| value)
        .collect();
    assert!(!assertions.is_empty(), "a presentation was sent");
    let key_body: String = material
        .holder_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let carriers = [
        ("the log", logs.text()),
        ("a metric", state.metrics().render()?),
        ("the answer", text),
    ];
    for (carrier, carried) in carriers {
        for part in material.credential.split('.').filter(|part| part.len() > 8) {
            assert!(!carried.contains(part), "{carrier} carries the credential");
        }
        for assertion in &assertions {
            for part in assertion.split('.').filter(|part| part.len() > 8) {
                assert!(
                    !carried.contains(part),
                    "{carrier} carries the presentation"
                );
            }
        }
        assert!(
            !carried.contains(&key_body),
            "{carrier} carries the holder key"
        );
    }
    Ok(())
}

/// The configuration refuses a Nuts grant beside another scheme, one whose
/// holder is no `did:web` DID, one with no `DPoP` key, and, outside the
/// development profile, one whose authorization server is plain `http`
/// (Annex B §B.4; Nuts RFC021 §7).
#[tokio::test]
async fn the_configuration_refuses_a_nuts_grant_it_cannot_use() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let resolve = |tables: &str| -> Result<ConfigError, Box<dyn Error>> {
        match Config::from_sources(Some(&signed(tables)), &BTreeMap::new())?.resolve() {
            Ok(_) => Err(format!("accepted: {tables}").into()),
            Err(error) => Ok(error),
        }
    };
    let both = material.tables.replace(
        "[[credentials.",
        "\n[credentials.\"node-a-pub\".oauth2]\nclient_id = \"x\"\n\n[[credentials.",
    );
    let error = resolve(&both)?;
    assert!(matches!(error, ConfigError::Scheme { .. }), "{error:?}");

    let not_web = material.tables.replace(
        &format!("did = \"{HOLDER}\""),
        "did = \"did:key:z6MkSynthetic\"",
    );
    let error = resolve(&not_web)?;
    assert!(matches!(error, ConfigError::NutsHolder { .. }), "{error:?}");

    let mut unbound = material
        .tables
        .lines()
        .filter(|line| !line.starts_with("dpop_key_file"))
        .collect::<Vec<_>>()
        .join("\n");
    unbound.push('\n');
    let error = resolve(&unbound)?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.nuts.dpop_key_file"),
        "{error:?}"
    );

    let production = format!("profile = \"production\"\n\n{}", material.tables);
    let settings = Config::from_sources(Some(&production), &BTreeMap::new())?.resolve()?;
    let Err(refused) = transport::check(&settings, None) else {
        return Err("a plain http authorization server passed the transport check".into());
    };
    assert_eq!(
        "credentials.node-a-pub.nuts.authorization_server",
        refused.site.url_key
    );
    let rendered = format!("{refused} {refused:?}");
    for part in material.credential.split('.').filter(|part| part.len() > 8) {
        assert!(
            !rendered.contains(part),
            "the refusal quotes the credential"
        );
    }
    Ok(())
}
