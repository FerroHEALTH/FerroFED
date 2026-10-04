// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FAPI 2.0 grant through the real configuration path: node A, whose
//! `[credentials]` name a `fapi2` grant, receives a `DPoP`-bound token the
//! harness FAPI 2.0 authorization server issued for an ES256 assertion it
//! verified against the client key the gateway publishes in its JWK Set; a
//! refused grant fails node A with nothing sent to it and no log, metric or
//! answer carrying an assertion or a key; and the configuration refuses a
//! grant it cannot use, the `oauth2` assertion audience included (§13.1,
//! §13.3, N25, CP-17; Annex B §B.4a; FAPI 2.0 Security Profile §5.3.2.1,
//! §5.3.3.1, §5.4.1, §5.4.2; RFC 9396; RFC 9449). Every identifier and key
//! is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_engine::onward::AssertionAudience;
use ferrofed_engine::onward::fapi2::Fapi2GrantError;
use ferrofed_engine::onward::keys::SigningKey;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::grant::GrantFault;
use ferrofed_server::config::settings::Scheme;
use ferrofed_server::config::transport;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, traced};
use ferrofed_testkit::fapi::{AuthorizationServer, DETAILS_TYPE};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth;
use ferrofed_testkit::unreachable;
use http::{Request, StatusCode, header};
use jsonwebtoken::jwk::JwkSet;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use crate::facade::{body, crossref, patient_query, post, registry, settings_with_room};
use crate::support::{Logs, bearer_as, call, signed};

type TestResult = Result<(), Box<dyn Error>>;

/// The gateway's client id at node A's authorization server, URA-shaped
/// under the example arc.
const CLIENT_ID: &str = "urn:oid:2.999.3.3.12345678";

/// The `authorization_details` node A's grant asks for, with synthetic
/// values.
const DETAILS: &str = r#"[{"type": "nl-gis-v1", "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT", "locations_organization_id": "urn:oid:2.999.3.3.87654321"}]"#;

const CALLER: &str = "synthetic-clinician-fapi2";
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// The `[credentials]` table of node A's grant, and its client key.
struct Material {
    tables: String,
    client_pem: String,
}

/// A harness FAPI 2.0 server requiring the details, and the
/// `[credentials]` table of node A's grant at it, its key files under
/// `dir`.
async fn material(dir: &Path) -> Result<(AuthorizationServer, Material), Box<dyn Error>> {
    let authority = AuthorizationServer::start(CLIENT_ID, Some(300)).await;
    authority
        .endpoint()
        .require_authorization_details([DETAILS_TYPE]);
    let client_pem = oauth::p256_pem()?;
    let file = |name: &str, contents: &str| -> Result<toml::Value, Box<dyn Error>> {
        let file = dir.join(name);
        std::fs::write(&file, contents)?;
        Ok(toml::Value::String(file.display().to_string()))
    };
    let client = file("fapi2-client.pem", &client_pem)?;
    let dpop = file("fapi2-dpop.pem", &oauth::p256_pem()?)?;
    let tables = format!(
        "[credentials.\"node-a-pub\".fapi2]\nissuer = \"{}\"\ngrant = \"client_credentials\"\nclient_id = \"{CLIENT_ID}\"\nclient_key_file = {client}\ndpop_key_file = {dpop}\nscope = \"system/aql-*.s\"\nauthorization_details = '''{DETAILS}'''\n",
        authority.issuer()
    );
    Ok((authority, Material { tables, client_pem }))
}

/// The gateway over node A at `a` and an unreachable node B, the patient
/// resolving at node A, with `tables`.
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

/// The JWK Set the gateway serves.
async fn published(app: &Router) -> Result<JwkSet, Box<dyn Error>> {
    let request = Request::get("/.well-known/jwks.json").body(Body::empty())?;
    let (status, text) = call(app.clone(), request).await?;
    assert_eq!(StatusCode::OK, status, "the JWK Set is served");
    Ok(serde_json::from_str(&text)?)
}

/// The patient query, sent as the test caller.
fn patient_post() -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, bearer_as(CALLER)?.parse()?);
    Ok(request)
}

/// The gateway publishes the grant's ES256 client key in its JWK Set, the
/// server verifies node A's assertion against that set, and node A
/// receives the `DPoP`-bound token (FAPI 2.0 §5.4.2; Annex B §B.4a.2).
// conformance: CP-17
#[tokio::test]
async fn node_a_receives_the_dpop_bound_fapi2_token() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (authority, material) = material(dir.path()).await?;
    let a = node(authority.endpoint().dpop_bound(None)).await;
    let (app, _state) = gateway(dir.path(), &a.uri(), &material.tables)?;
    let jwks = published(&app).await?;
    let client_key = SigningKey::from_p256_pem(&SecretString::from(material.client_pem))?;
    assert!(
        jwks.find(client_key.kid()).is_some(),
        "the client key is published"
    );
    authority.endpoint().trust(jwks);

    let (status, text) = call(app, patient_post()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, authority.endpoint().issued());
    Ok(())
}

/// A refused grant fails node A with nothing sent to it, and no log line,
/// metric or answer carries an assertion or the client key (§11, N25).
// conformance: CP-17
#[tokio::test]
async fn a_refused_fapi2_grant_fails_node_a_and_names_no_credential() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (authority, material) = material(dir.path()).await?;
    authority
        .endpoint()
        .refuse(401, "invalid_client", "the assertion was refused");
    let a = node(authority.endpoint().dpop_bound(None)).await;
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
        .endpoint()
        .forms()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name == "client_assertion")
        .map(|(_, value)| value)
        .collect();
    assert!(!assertions.is_empty(), "an assertion was sent");
    let key_body: String = material
        .client_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let carriers = [
        ("the log", logs.text()),
        ("a metric", state.metrics().render()?),
        ("the answer", text),
    ];
    for (carrier, carried) in carriers {
        for assertion in &assertions {
            for part in assertion.split('.').filter(|part| part.len() > 8) {
                assert!(!carried.contains(part), "{carrier} carries the assertion");
            }
        }
        assert!(
            !carried.contains(&key_body),
            "{carrier} carries the client key"
        );
    }
    Ok(())
}

/// Resolves `tables`, which must be refused, and returns the refusal.
fn refused(tables: &str) -> Result<ConfigError, Box<dyn Error>> {
    match Config::from_sources(Some(&signed(tables)), &BTreeMap::new())?.resolve() {
        Ok(_) => Err(format!("accepted: {tables}").into()),
        Err(error) => Ok(error),
    }
}

/// `tables` with the line starting `key` replaced by `line`, or removed when
/// `line` is empty.
fn with_line(tables: &str, key: &str, line: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for current in tables.lines() {
        if current.starts_with(key) {
            if !line.is_empty() {
                lines.push(line.to_owned());
            }
        } else {
            lines.push(current.to_owned());
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// The configuration refuses a FAPI 2.0 grant it cannot use: beside another
/// scheme, the authorization code grant, keys that do not sign ES256, no
/// `DPoP` key, details outside RFC 9396 §2, nothing asked for, an exchange
/// without a resource, and an issuer outside RFC 8414 §2.
#[tokio::test]
async fn the_configuration_refuses_a_fapi2_grant_it_cannot_use() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let tables = &material.tables;

    let both = format!("{tables}\n[credentials.\"node-a-pub\".oauth2]\nclient_id = \"x\"\n");
    let error = refused(&both)?;
    assert!(matches!(error, ConfigError::Scheme { .. }), "{error:?}");

    let error = refused(&with_line(
        tables,
        "grant",
        "grant = \"authorization_code\"",
    ))?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::UserAgentFlow { .. })
        ),
        "{error:?}"
    );

    let p384 = dir.path().join("p384.pem");
    std::fs::write(&p384, oauth::es384_pem()?)?;
    let p384 = toml::Value::String(p384.display().to_string());
    let error = refused(&with_line(
        tables,
        "client_key_file",
        &format!("client_key_file = {p384}"),
    ))?;
    assert!(
        matches!(error, ConfigError::GrantFault(GrantFault::ClientKey { .. })),
        "{error:?}"
    );
    let error = refused(&with_line(
        tables,
        "dpop_key_file",
        &format!("dpop_key_file = {p384}"),
    ))?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::Fapi2 {
                source: Fapi2GrantError::DpopKey,
                ..
            })
        ),
        "{error:?}"
    );

    let error = refused(&with_line(tables, "dpop_key_file", ""))?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.fapi2.dpop_key_file"),
        "{error:?}"
    );

    let error = refused(&with_line(
        tables,
        "authorization_details",
        "authorization_details = '[{\"actions\": [\"read\"]}]'",
    ))?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::AuthorizationDetails { .. })
        ),
        "{error:?}"
    );

    let unrequested = with_line(&with_line(tables, "scope", ""), "authorization_details", "");
    let error = refused(&unrequested)?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::Fapi2 {
                source: Fapi2GrantError::Unrequested,
                ..
            })
        ),
        "{error:?}"
    );

    let error = refused(&with_line(tables, "grant", "grant = \"token_exchange\""))?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.fapi2.resource"),
        "{error:?}"
    );

    let error = refused(&with_line(
        tables,
        "issuer",
        "issuer = \"https://AS.example.org/tenant?x=1\"",
    ))?;
    assert!(
        matches!(error, ConfigError::GrantFault(GrantFault::Issuer { .. })),
        "{error:?}"
    );
    Ok(())
}

/// A FAPI 2.0 grant needs `[signing]`, whose JWK Set publishes its client
/// key (FAPI 2.0 §5.4.2).
#[tokio::test]
async fn a_fapi2_grant_without_signing_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let error = refused(&material.tables)?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::WithoutSigning { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

/// Outside the development profile, an issuer that is plain `http` fails the
/// transport check (FAPI 2.0 §5.2.1).
#[tokio::test]
async fn a_cleartext_issuer_is_refused_outside_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_authority, material) = material(dir.path()).await?;
    let production = format!(
        "profile = \"production\"\n\n{}{}",
        material.tables,
        crate::support::signing_toml()
    );
    let settings = Config::from_sources(Some(&production), &BTreeMap::new())?.resolve()?;
    let Err(refused) = transport::check(&settings, None) else {
        return Err("a plain http issuer passed the transport check".into());
    };
    assert_eq!("credentials.node-a-pub.fapi2.issuer", refused.site.url_key);
    Ok(())
}

/// The `oauth2` grant names the token endpoint as its assertions' `aud` by
/// default, and the issuer with `assertion_audience = "issuer"`, which then
/// needs `issuer`; an `issuer` beside the default is refused (RFC 7523 §3;
/// FAPI 2.0 §5.3.2.1).
#[test]
fn the_oauth2_assertion_audience_is_configured() -> TestResult {
    let oauth2 = |extra: &str| {
        format!(
            "[credentials.\"node-a-pub\".oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"gateway\"\nscope = \"system/aql-*.s\"\n{extra}{}",
            crate::support::signing_toml()
        )
    };
    let aud = |text: &str| -> Result<(AssertionAudience, String), Box<dyn Error>> {
        let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
        match settings.credentials.values().next() {
            Some(Scheme::OAuth2(grant)) => Ok((
                grant.assertion_audience().clone(),
                grant.assertion_aud().to_owned(),
            )),
            other => Err(format!("an oauth2 grant, got {other:?}").into()),
        }
    };
    let (audience, written) = aud(&oauth2(""))?;
    assert_eq!(AssertionAudience::TokenEndpoint, audience);
    assert_eq!("https://as.example.org/token", written);
    let (_, written) = aud(&oauth2(
        "assertion_audience = \"issuer\"\nissuer = \"https://as.example.org\"\n",
    ))?;
    assert_eq!("https://as.example.org", written);

    let error = refused(&oauth2("assertion_audience = \"issuer\"\n"))?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.oauth2.issuer"),
        "{error:?}"
    );
    let error = refused(&oauth2("issuer = \"https://as.example.org\"\n"))?;
    assert!(
        matches!(
            error,
            ConfigError::GrantFault(GrantFault::IssuerUnused { .. })
        ),
        "{error:?}"
    );
    Ok(())
}
