// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant through the real configuration path: node A, whose
//! `[credentials]` name a Nuts grant, receives a token the harness Nuts node
//! issued for a Verifiable Presentation of the gateway's credentials,
//! `DPoP`-bound to the grant's key; a refused grant fails node A with
//! nothing sent to it and no log, span or answer carrying a credential, the
//! presentation or a key; and the configuration refuses a grant it cannot
//! use (§13.1, §13.3, N25, CP-17; Annex B §B.4; Nuts RFC021; RFC 9449).
//! The gateway serves its holder's `did:web` DID document, which verifies
//! the presentation it made. Every identifier, key and credential is
//! synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the served DID document and the presentation are read as values"
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
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use crate::facade::{body, crossref, patient_query, post, registry, settings_with_room};
use crate::support::{Logs, bearer_as, call, send_as_is, signed};

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
    material_as(dir, "node-a-pub", (HOLDER, HOLDER_KID)).await
}

/// A harness Nuts node trusting a fresh holder `did`, whose key `kid`
/// names, and a fresh issuer, and the `[credentials]` table of `endpoint`'s
/// grant at it, its files under `dir` named for `endpoint`.
async fn material_as(
    dir: &Path,
    endpoint: &str,
    (did, kid): (&str, &str),
) -> Result<(NutsNode, Material), Box<dyn Error>> {
    let authority = NutsNode::start("hospital-a", SCOPE, Some(300)).await;
    authority.define(DEFINITION);
    let holder_pem = oauth::p256_pem()?;
    let issuer_pem = oauth::p256_pem()?;
    authority.trust_holder(did, kid, nuts::public_jwk(&holder_pem, Algorithm::ES256)?);
    authority.trust_issuer(
        ISSUER,
        ISSUER_KID,
        nuts::public_jwk(&issuer_pem, Algorithm::ES256)?,
    );
    let credential = nuts::credential(
        (&issuer_pem, ISSUER_KID),
        ISSUER,
        did,
        (
            "SyntheticOrganizationCredential",
            "Synthetic Care Organisation",
        ),
        jiff::Timestamp::now().as_second() + 3600,
    )?;
    let file = |name: &str, contents: &str| -> Result<toml::Value, Box<dyn Error>> {
        let file = dir.join(format!("{endpoint}-{name}"));
        std::fs::write(&file, contents)?;
        Ok(toml::Value::String(file.display().to_string()))
    };
    let key = file("holder.pem", &holder_pem)?;
    let dpop = file("dpop.pem", &oauth::p256_pem()?)?;
    let held = file("organization.jwt", &credential)?;
    let tables = format!(
        "[credentials.\"{endpoint}\".nuts]\nauthorization_server = \"{}\"\nscope = \"{SCOPE}\"\ndid = \"{did}\"\nkid = \"{kid}\"\nkey_file = {key}\ndpop_key_file = {dpop}\n\n[[credentials.\"{endpoint}\".nuts.credential]]\ninput_descriptor = \"organization_credential\"\nfile = {held}\n",
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

/// The DID document `app` serves at `path`, fetched with no credential, and
/// its media type.
async fn did_document(app: &Router, path: &str) -> Result<(Value, String), Box<dyn Error>> {
    let request = Request::get(path).body(axum::body::Body::empty())?;
    let response = send_as_is(app.clone(), request).await?;
    let status = response.status();
    let media = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    if status != StatusCode::OK {
        return Err(format!("{path} answered {status}").into());
    }
    Ok((serde_json::from_slice(&bytes)?, media))
}

/// `error` and every cause behind it, as one line.
fn chain(error: &(dyn Error + 'static)) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

/// The one verification method of `document`, with its public JWK.
fn only_method(document: &Value) -> Result<(String, Jwk), Box<dyn Error>> {
    let methods = document["verificationMethod"]
        .as_array()
        .ok_or("verificationMethod")?;
    let [method] = methods.as_slice() else {
        return Err(format!("one verification method: {methods:?}").into());
    };
    let id = method["id"].as_str().ok_or("an id")?.to_owned();
    Ok((id, serde_json::from_value(method["publicKeyJwk"].clone())?))
}

/// The gateway serves its `did:web` DID document at the location the DID
/// names, with no client credential, built from the key it signs
/// presentations with and nothing else; the authorization server, trusting
/// the key the served document holds, verifies the presentation the gateway
/// made, and node A receives the token (the did:web Method Specification,
/// Read (Resolve); Nuts RFC021 §4.2 item 4; Annex B §B.4).
// conformance: CP-17
#[tokio::test]
async fn the_served_did_document_verifies_the_gateways_presentation() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (authority, material) = material(dir.path()).await?;
    let a = node(authority.dpop_bound()).await;
    let (app, _state) = gateway(dir.path(), &a.uri(), &material.tables)?;

    let (document, media) = did_document(&app, "/.well-known/did.json").await?;
    assert_eq!("application/did+ld+json", media);
    assert_eq!(HOLDER, document["id"]);
    let (kid, jwk) = only_method(&document)?;
    assert_eq!(HOLDER_KID, kid);
    assert_eq!(
        nuts::public_jwk(&material.holder_pem, Algorithm::ES256)?
            .common
            .key_algorithm,
        jwk.common.key_algorithm
    );
    let served = serde_json::to_value(&jwk)?;
    assert!(served.get("d").is_none(), "no private member is served");
    let key_body: String = material
        .holder_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    assert!(!document.to_string().contains(&key_body));

    authority.trust_holder(HOLDER, HOLDER_KID, jwk.clone());
    let (status, text) = call(app, patient_post()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, authority.issued());
    let forms = authority.forms();
    let presentation = forms
        .first()
        .and_then(|form| form.iter().find(|(name, _)| name == "assertion"))
        .map(|(_, value)| value.clone())
        .ok_or("the presentation")?;
    let header = jsonwebtoken::decode_header(&presentation)?;
    assert_eq!(Some(HOLDER_KID), header.kid.as_deref());
    let mut validation = Validation::new(header.alg);
    validation.validate_aud = false;
    jsonwebtoken::decode::<Value>(&presentation, &DecodingKey::from_jwk(&jwk)?, &validation)?;
    Ok(())
}

/// A key change is served with no hand-edited file: the gateway started
/// over a new holder key serves a document holding that key.
#[tokio::test]
async fn a_new_holder_key_is_served_in_the_did_document() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let (app, _state) = gateway(
        dir.path(),
        "https://cdr-a.example.org/openehr",
        &material.tables,
    )?;
    let (before, _) = did_document(&app, "/.well-known/did.json").await?;
    let new_pem = oauth::p256_pem()?;
    std::fs::write(dir.path().join("node-a-pub-holder.pem"), &new_pem)?;
    let (app, _state) = gateway(
        dir.path(),
        "https://cdr-a.example.org/openehr",
        &material.tables,
    )?;
    let (after, _) = did_document(&app, "/.well-known/did.json").await?;
    let (_, old_jwk) = only_method(&before)?;
    let (_, new_jwk) = only_method(&after)?;
    assert_ne!(old_jwk, new_jwk);
    let mut expected = nuts::public_jwk(&new_pem, Algorithm::ES256)?;
    expected.common.key_id = None;
    assert_eq!(
        serde_json::to_value(&expected)?,
        serde_json::to_value(&new_jwk)?
    );
    Ok(())
}

/// The document's path is the one the DID names on its host, so a gateway
/// mounted under a base other than `/` serves it there too, outside the
/// base, and answers a path under the base the DID does not name `404`.
#[tokio::test]
async fn the_did_document_is_served_at_the_dids_own_path_under_any_base() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let (_app, state) = gateway(
        dir.path(),
        "https://cdr-a.example.org/openehr",
        &material.tables,
    )?;
    let mut server = settings_with_room();
    server.base_path = "/fed".parse()?;
    let app = ferrofed_server::router(state, &server);
    let (document, _) = did_document(&app, "/.well-known/did.json").await?;
    assert_eq!(HOLDER, document["id"]);
    let request = Request::get("/fed/.well-known/did.json").body(axum::body::Body::empty())?;
    let response = send_as_is(app, request).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

/// A DID whose document path lies under the ITS-REST surface, and two DIDs
/// whose documents share one path, refuse to load: the document stays
/// outside the client authentication gate, and one path serves one
/// document.
#[tokio::test]
async fn a_did_document_inside_the_surface_or_at_a_taken_path_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let surface = "did:web:gateway.example.org:v1:holder";
    let (_authority, inside) = material_as(
        dir.path(),
        "node-a-pub",
        (surface, &format!("{surface}#key-1")),
    )
    .await?;
    let refused = gateway(
        dir.path(),
        "https://cdr-a.example.org/openehr",
        &inside.tables,
    )
    .err()
    .ok_or("a document inside the surface loaded")?;
    assert!(
        chain(refused.as_ref()).contains("ITS-REST surface"),
        "{refused}"
    );

    let (_authority, a) = material(dir.path()).await?;
    let other = "did:web:other.example.org";
    let (_authority, b) =
        material_as(dir.path(), "node-b-pub", (other, &format!("{other}#key-1"))).await?;
    let refused = gateway(
        dir.path(),
        "https://cdr-a.example.org/openehr",
        &format!("{}\n{}", a.tables, b.tables),
    )
    .err()
    .ok_or("two documents at one path loaded")?;
    assert!(
        chain(refused.as_ref()).contains("/.well-known/did.json"),
        "{refused}"
    );
    Ok(())
}
