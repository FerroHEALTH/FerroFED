// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer authenticated with the Nuts grant, through the real
//! configuration path: `[nl_gf.nvi.credentials.nuts]` takes the table a
//! node's onward credentials take, the harness Nuts node issues a
//! `DPoP`-bound token for a Verifiable Presentation of the gateway's
//! credentials, and the harness Localization Service answers only that token
//! with a proof of the grant's key (the IG's Localization page,
//! GF-Authentication, GFI-004 and GFI-005; Annex B §B.1, §B.4; Nuts RFC021;
//! RFC 9449 §7.1). A refused grant fails the localization closed (§14.1);
//! a token the service refuses, or one near the end of its lifetime, is
//! replaced; the token reaches the NVI alone, never a node, a log or an
//! answer; and the configuration refuses a credential the IG does not give
//! the Localization Service, naming its key. Every identifier, key and
//! credential is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::transport;
use ferrofed_server::state::AppState;
use ferrofed_server::telemetry::{Rendering, traced};
use ferrofed_testkit::dpop;
use ferrofed_testkit::nuts::{self, NutsNode};
use ferrofed_testkit::nvi::LocalizationService;
use ferrofed_testkit::oauth;
use http::StatusCode;
use jsonwebtoken::Algorithm;

use crate::facade::{Answer, body, post, settings_with_room, statuses, wire};
use crate::nl_gf::{URAS, asked_counts, configuration, fed_manager, members, patient, query};
use crate::support::{Logs, call, signed};

type TestResult = Result<(), Box<dyn Error>>;

const HOLDER: &str = "did:web:gateway.example.org";
const HOLDER_KID: &str = "did:web:gateway.example.org#key-1";
const ISSUER: &str = "did:web:issuer.example.org";
const ISSUER_KID: &str = "did:web:issuer.example.org#key-1";
const SCOPE: &str = "nl-gf-localization";
const DEFINITION: &str =
    r#"{"id": "pd_synthetic", "input_descriptors": [{"id": "organization_credential"}]}"#;

/// The section the grant is configured in.
const SECTION: &str = "nl_gf.nvi.credentials.nuts";

/// A harness Nuts node trusting a fresh holder and issuer, issuing tokens
/// that live `expires_in` seconds.
struct Grant {
    authority: NutsNode,
    tables: String,
    credential: String,
    holder_pem: String,
}

async fn grant(dir: &Path, expires_in: u64) -> Result<Grant, Box<dyn Error>> {
    let authority = NutsNode::start("nvi", SCOPE, Some(expires_in)).await;
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
        "\n[{SECTION}]\nauthorization_server = \"{}\"\nscope = \"{SCOPE}\"\ndid = \"{HOLDER}\"\nkid = \"{HOLDER_KID}\"\nkey_file = {key}\ndpop_key_file = {dpop}\n\n[[{SECTION}.credential]]\ninput_descriptor = \"organization_credential\"\nfile = {held}\n",
        authority.issuer()
    );
    Ok(Grant {
        authority,
        tables,
        credential,
        holder_pem,
    })
}

/// The gateway over the members at `urls`, localized by the NVI at `nvi`
/// with `tables` added, and resolved by the PIX Manager at `manager`.
fn gateway(
    dir: &Path,
    urls: [&str; 3],
    (nvi, manager): (&str, &str),
    tables: &str,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let text = configuration(dir, "development", urls, nvi, manager)? + tables;
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok((
        ferrofed_server::router(Arc::clone(&state), &settings_with_room()),
        state,
    ))
}

/// The harness NVI answering only the token `authority` issued, with the
/// patient's data held by the first two care providers.
async fn guarded_nvi(authority: &NutsNode) -> LocalizationService {
    let nvi = LocalizationService::start_guarded(authority.dpop_bound()).await;
    nvi.index(&patient().value(), URAS[0]);
    nvi.index(&patient().value(), URAS[1]);
    nvi
}

/// Runs the patient query and returns the status, the body and the answer.
async fn ask(app: &Router) -> Result<(StatusCode, String, Answer), Box<dyn Error>> {
    let (status, text) = call(app.clone(), post(body(&query())?)?).await?;
    let answer: Answer = serde_json::from_str(&text)?;
    Ok((status, text, answer))
}

/// The NVI is asked with the token the Nuts grant obtained, and the members
/// it names are asked by their own `ehr_id` (Annex B §B.1, §B.4; GFI-005).
// conformance: CP-5
#[tokio::test]
async fn the_nvi_is_asked_with_the_dpop_bound_nuts_token() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;
    let nvi = guarded_nvi(&grant.authority).await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let (app, _state) = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        (&nvi.base(), &pix.base_url()),
        &grant.tables,
    )?;

    let (status, text, answer) = ask(&app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "not-localized"),
        ],
        statuses(&answer)
    );
    assert_eq!(1, grant.authority.issued());
    let [authorization] = <[String; 1]>::try_from(nvi.headers("authorization").await)
        .map_err(|all| format!("one search, not {}", all.len()))?;
    assert!(authorization.starts_with("DPoP "), "the DPoP scheme");
    Ok(())
}

/// A grant the authorization server refuses leaves the localization
/// unavailable, so it fails closed: every member is `not-localized` with the
/// error, nothing reaches the NVI, the PIX Manager or a node, and no log
/// line, metric or answer names the credential, the presentation, the
/// holder key or the pseudonym (§14.1; Nuts RFC021 §4.5).
// conformance: CP-5
#[tokio::test]
async fn a_refused_nuts_grant_fails_the_localization_closed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;
    grant
        .authority
        .refuse(400, "invalid_request", "the presentation was refused");
    let nvi = guarded_nvi(&grant.authority).await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let (app, state) = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        (&nvi.base(), &pix.base_url()),
        &grant.tables,
    )?;

    let logs = Logs::default();
    let subscriber = traced(Rendering::Json, "trace", false, logs.clone(), None)?;
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, text, answer) = ask(&app).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");
    for endpoint in &answer.meta.federation.endpoints {
        assert_eq!("not-localized", endpoint.status);
        assert!(endpoint.error.is_some(), "the localization error (§14.1)");
    }
    assert!(nvi.searches().await.is_empty(), "the NVI was sent nothing");
    assert_eq!([0, 0, 0], asked_counts(&servers).await?, "no dispatch");
    assert_eq!(0, pix.queries(), "nothing was resolved");

    let assertions: Vec<String> = grant
        .authority
        .forms()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name == "assertion")
        .map(|(_, value)| value)
        .collect();
    assert!(!assertions.is_empty(), "a presentation was sent");
    let key_body: String = grant
        .holder_pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let errors = serde_json::to_string(
        &answer
            .meta
            .federation
            .endpoints
            .iter()
            .map(|endpoint| &endpoint.error)
            .collect::<Vec<_>>(),
    )?;
    // The answer echoes the client's own query, the pseudonym in it, as `q`.
    let carriers = [
        ("the log", logs.text(), true),
        ("a metric", state.metrics().render()?, true),
        ("an endpoint error", errors, true),
        ("the answer", text, false),
    ];
    for (carrier, carried, pseudonym_free) in carriers {
        for part in grant.credential.split('.').filter(|part| part.len() > 8) {
            assert!(!carried.contains(part), "{carrier} carries the credential");
        }
        for part in assertions
            .iter()
            .flat_map(|assertion| assertion.split('.'))
            .filter(|part| part.len() > 8)
        {
            assert!(
                !carried.contains(part),
                "{carrier} carries the presentation"
            );
        }
        assert!(
            !carried.contains(&key_body),
            "{carrier} carries the holder key"
        );
        assert!(
            !(pseudonym_free && carried.contains(&patient().value())),
            "{carrier} carries the pseudonym"
        );
    }
    Ok(())
}

/// The token reaches the NVI alone: no node is sent it, its scheme or a
/// proof, no log line or answer carries it, and the pseudonym reaches
/// neither a node nor the authorization server (§5.4, N33; Annex B §B.7).
// conformance: CP-26
#[tokio::test]
async fn the_nuts_token_reaches_the_nvi_alone() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;
    let nvi = guarded_nvi(&grant.authority).await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let (app, state) = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        (&nvi.base(), &pix.base_url()),
        &grant.tables,
    )?;

    let logs = Logs::default();
    let subscriber = traced(Rendering::Json, "trace", false, logs.clone(), None)?;
    let guard = tracing::subscriber::set_default(subscriber);
    let (status, text, _answer) = ask(&app).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "{text}");

    let tokens: Vec<String> = nvi
        .headers("authorization")
        .await
        .iter()
        .filter_map(|value| value.strip_prefix("DPoP ").map(str::to_owned))
        .collect();
    assert_eq!(1, tokens.len(), "the NVI was sent the token");
    let proofs = nvi.headers(dpop::HEADER).await;
    let pseudonym = patient().value();
    for server in &servers {
        let wire = wire(server).await?;
        for token in &tokens {
            assert!(!wire.contains(token), "a node was sent the NVI's token");
        }
        for proof in &proofs {
            assert!(!wire.contains(proof), "a node was sent the NVI's proof");
        }
        assert!(!wire.contains(&pseudonym), "a node was sent the pseudonym");
    }
    for (carrier, carried) in [
        ("the log", logs.text()),
        ("a metric", state.metrics().render()?),
        ("the answer", text),
    ] {
        for token in &tokens {
            assert!(
                !carried.contains(token.as_str()),
                "{carrier} carries the token"
            );
        }
    }
    for (name, value) in grant.authority.forms().into_iter().flatten() {
        assert!(
            !value.contains(&pseudonym),
            "the {name} form names the pseudonym"
        );
    }
    Ok(())
}

/// A token the NVI no longer accepts fails that query's localization
/// closed, and is dropped, so the next query obtains another and localizes.
#[tokio::test]
async fn a_token_the_nvi_refuses_is_replaced_on_the_next_query() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;
    let nvi = guarded_nvi(&grant.authority).await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let (app, _state) = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        (&nvi.base(), &pix.base_url()),
        &grant.tables,
    )?;
    ask(&app).await?;
    grant.authority.revoke_all();

    let (_status, _text, refused) = ask(&app).await?;
    assert!(
        refused
            .meta
            .federation
            .endpoints
            .iter()
            .all(|endpoint| endpoint.status == "not-localized" && endpoint.error.is_some()),
        "the refused token fails the localization closed (§14.1)"
    );
    let (_status, _text, replaced) = ask(&app).await?;
    assert_eq!(
        vec![
            ("node-a-pub", "active"),
            ("node-b-pub", "active"),
            ("node-c-pub", "not-localized"),
        ],
        statuses(&replaced)
    );
    assert_eq!(2, grant.authority.issued(), "a new token after the 401");
    Ok(())
}

/// A token is kept until 30 seconds before the lifetime it was issued with
/// ends: one that lives 31 seconds serves the queries of the next second,
/// and a query after that obtains a new one.
#[tokio::test]
async fn a_token_near_the_end_of_its_lifetime_is_replaced() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 31).await?;
    let nvi = guarded_nvi(&grant.authority).await;
    let pix = fed_manager().await?;
    let servers = members().await;
    let (app, _state) = gateway(
        dir.path(),
        [&servers[0].uri(), &servers[1].uri(), &servers[2].uri()],
        (&nvi.base(), &pix.base_url()),
        &grant.tables,
    )?;

    ask(&app).await?;
    ask(&app).await?;
    assert_eq!(1, grant.authority.issued(), "the token is cached");
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let (_status, _text, answer) = ask(&app).await?;
    assert_eq!(("node-a-pub", "active"), statuses(&answer)[0]);
    assert_eq!(2, grant.authority.issued(), "a new token near the end");
    Ok(())
}

/// What the gateway's configuration under `profile`, every URL `https` and
/// `tables` added, resolves to.
fn resolve(
    dir: &Path,
    profile: &str,
    tables: &str,
) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    let text = configuration(
        dir,
        profile,
        [
            "https://a.example.org",
            "https://b.example.org",
            "https://c.example.org",
        ],
        "https://nvi.example.org/fhir",
        "https://pix.example.org/fhir/",
    )? + tables;
    Ok(Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve())
}

/// An OAuth 2.0 or FAPI 2.0 grant is refused for the NVI, naming its table:
/// the IG authenticates the data user of the Localization Service on
/// GF-Authentication alone (the IG's Localization page, Authentication and
/// Authorization).
#[test]
fn an_oauth2_or_fapi2_grant_for_the_nvi_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    for (grant, table) in [
        ("oauth2", "client_id = \"x\"\nscope = \"system/aql-*.s\"\n"),
        (
            "fapi2",
            "issuer = \"https://as.example.org\"\nscope = \"x\"\n",
        ),
    ] {
        let tables = format!("\n[nl_gf.nvi.credentials.{grant}]\n{table}");
        match resolve(dir.path(), "development", &tables)? {
            Err(ConfigError::NviGrant { key }) => {
                assert_eq!(format!("nl_gf.nvi.credentials.{grant}"), key);
            }
            other => return Err(format!("{grant} for the NVI: {other:?}").into()),
        }
    }
    Ok(())
}

/// The Nuts grant beside a bearer token is refused, and a grant missing a
/// value names its key (Annex B §B.4).
#[tokio::test]
async fn a_nuts_grant_the_nvi_cannot_use_names_its_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;

    let both = format!(
        "\n[nl_gf.nvi.credentials]\nbearer_token = \"synthetic-token\"\n{}",
        grant.tables
    );
    match resolve(dir.path(), "development", &both)? {
        Err(ConfigError::Scheme { section }) => assert_eq!("nl_gf.nvi.credentials", section),
        other => return Err(format!("two schemes: {other:?}").into()),
    }

    let mut unbound = grant
        .tables
        .lines()
        .filter(|line| !line.starts_with("dpop_key_file"))
        .collect::<Vec<_>>()
        .join("\n");
    unbound.push('\n');
    match resolve(dir.path(), "development", &unbound)? {
        Err(ConfigError::Missing { key }) => assert_eq!(format!("{SECTION}.dpop_key_file"), key),
        other => return Err(format!("no DPoP key: {other:?}").into()),
    }

    let not_web = grant.tables.replace(
        &format!("did = \"{HOLDER}\""),
        "did = \"did:key:z6MkSynthetic\"",
    );
    match resolve(dir.path(), "development", &not_web)? {
        Err(ConfigError::NutsHolder { section, .. }) if section == SECTION => {}
        other => return Err(format!("a holder that is no did:web: {other:?}").into()),
    }
    Ok(())
}

/// Outside the development profile, an authorization server over plain
/// `http` is refused, naming its key, and the refusal quotes no credential
/// (Nuts RFC021 §7).
#[tokio::test]
async fn a_plain_http_authorization_server_for_the_nvi_is_refused_in_production() -> TestResult {
    let dir = tempfile::tempdir()?;
    let grant = grant(dir.path(), 300).await?;
    let settings = resolve(dir.path(), "production", &grant.tables)??;
    let Err(refused) = transport::check(&settings, None) else {
        return Err("a plain http authorization server passed the transport check".into());
    };
    assert_eq!(
        format!("{SECTION}.authorization_server"),
        refused.site.url_key
    );
    let rendered = format!("{refused} {refused:?}");
    for part in grant.credential.split('.').filter(|part| part.len() > 8) {
        assert!(
            !rendered.contains(part),
            "the refusal quotes the credential"
        );
    }
    Ok(())
}
