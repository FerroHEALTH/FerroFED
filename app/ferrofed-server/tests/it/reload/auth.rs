// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reload puts `[auth]` in place: an issuer it adds is trusted for every
//! request that arrives afterwards, a request already being admitted
//! finishes on the issuers it started with, and an `[auth]` that does not
//! load, or trusts no issuer, is refused with the whole reload and the
//! running issuers kept (§13.1, N25, on who is admitted; no specification
//! governs reloading: our own design).

use std::error::Error;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ferrofed_server::auth::trust::Trust;
use ferrofed_server::config::Config;
use ferrofed_server::reload::Reloader;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{Claims, Issuer, METADATA_PATH};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use tower::ServiceExt as _;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{TestResult, configuration, member};
use crate::facade::settings_with_room;
use crate::support::{self, AUDIENCE, call};

/// The issuer a reload adds to the trust list.
const ISSUER_B: &str = "https://issuer-b.example.test";

/// The issuer a reload adds, its key generated once per test process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static SECOND: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(ISSUER_B).expect("a test key pair should generate"));

/// The `Authorization` value of a token [`SECOND`] signed for this gateway.
pub(super) fn bearer_b() -> Result<String, Box<dyn Error>> {
    Ok(format!(
        "Bearer {}",
        SECOND.mint(&Claims::new(ISSUER_B, AUDIENCE))?
    ))
}

/// `[auth]` trusting the suite's issuer, with the product scope that admits
/// operators.
pub(super) fn auth_a() -> String {
    support::auth_toml()
}

/// `[auth]` trusting the suite's issuer and [`SECOND`], its key set in a
/// file.
pub(super) fn auth_a_and_b() -> Result<String, Box<dyn Error>> {
    Ok(format!("{}{}", auth_a(), support::issuer_toml(&SECOND)?))
}

/// A development gateway over one member whose client face, admin listener
/// and reloader share one gate, as `ferrofed serve` wires them.
pub(super) struct Trusting {
    _dir: tempfile::TempDir,
    config: PathBuf,
    document: PathBuf,
    pub(super) state: Arc<AppState>,
    pub(super) trust: Arc<Trust>,
    pub(super) reloader: Arc<Reloader>,
    pub(super) app: Router,
}

impl Trusting {
    /// Starts the gateway with `auth` appended to its configuration.
    pub(super) fn start(auth: &str) -> Result<Self, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let config = dir.path().join("ferrofed.toml");
        let document = dir.path().join("registry.toml");
        std::fs::write(&document, member("a", "http://127.0.0.1:9/a"))?;
        std::fs::write(&config, configuration(&document, "", auth))?;
        let settings = Config::load(Some(&config))?.resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        let trust = Arc::new(Trust::new(&settings.server.auth));
        let app = ferrofed_server::trusted_router(
            Arc::clone(&state),
            &settings_with_room(),
            Arc::clone(&trust),
        );
        let reloader = Arc::new(
            Reloader::new(Some(config.clone()), settings, Arc::clone(&state))
                .with_trust(Arc::clone(&trust)),
        );
        Ok(Self {
            _dir: dir,
            config,
            document,
            state,
            trust,
            reloader,
            app,
        })
    }

    /// Rewrites the configuration with `auth` appended.
    pub(super) fn write_auth(&self, auth: &str) -> std::io::Result<()> {
        std::fs::write(&self.config, configuration(&self.document, "", auth))
    }

    /// Rewrites the registry document.
    fn write_registry(&self, registry: &str) -> std::io::Result<()> {
        std::fs::write(&self.document, registry)
    }

    /// How many members the running registry holds.
    fn members(&self) -> Result<usize, &'static str> {
        let federation = self.state.federation().ok_or("a registry is configured")?;
        Ok(federation.snapshot().nodes().count())
    }
}

/// `OPTIONS {base}/` with `authorization`, which client authentication
/// guards and which the federation answers `200` once admitted.
pub(super) fn described(authorization: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::options("/")
        .header(header::AUTHORIZATION, authorization)
        .body(Body::empty())?)
}

/// The status `app` answers `OPTIONS {base}/` with as the caller of
/// `authorization`.
pub(super) async fn status_of(
    app: &Router,
    authorization: &str,
) -> Result<StatusCode, Box<dyn Error>> {
    let (status, _) = call(app.clone(), described(authorization)?).await?;
    Ok(status)
}

#[tokio::test]
async fn a_reload_adds_an_issuer_whose_tokens_are_admitted_after_it() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    let b = bearer_b()?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_of(&gateway.app, &b).await?,
        "before the reload the issuer is not trusted"
    );

    gateway.write_auth(&auth_a_and_b()?)?;
    let applied = gateway.reloader.reload()?;
    assert!(applied.auth_changed, "{applied:?}");
    assert!(applied.needs_restart.is_empty(), "{applied:?}");

    assert_eq!(StatusCode::OK, status_of(&gateway.app, &b).await?);
    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &support::bearer()?).await?,
        "the issuer that stayed is still trusted"
    );
    Ok(())
}

#[tokio::test]
async fn a_reload_that_drops_an_issuer_refuses_its_tokens_after_it() -> TestResult {
    let gateway = Trusting::start(&auth_a_and_b()?)?;
    let b = bearer_b()?;
    assert_eq!(StatusCode::OK, status_of(&gateway.app, &b).await?);

    gateway.write_auth(&auth_a())?;
    gateway.reloader.reload()?;
    assert_eq!(StatusCode::UNAUTHORIZED, status_of(&gateway.app, &b).await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_being_admitted_finishes_on_the_issuers_it_started_with() -> TestResult {
    let keys = Server::start().await;
    let slow = Issuer::new(keys.uri())?;
    let metadata = serde_json::json!({
        "issuer": slow.name(),
        "jwks_uri": format!("{}/jwks", keys.uri()),
    });
    Mock::given(method("GET"))
        .and(path(METADATA_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
        .mount(&keys)
        .await;
    Mock::given(method("GET"))
        .and(path("/jwks"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(slow.jwks_json()?, "application/json")
                .set_delay(Duration::from_millis(1_500)),
        )
        .mount(&keys)
        .await;
    let fetched = format!(
        "{}\n[[auth.issuer]]\nissuer = \"{}\"\n",
        auth_a(),
        slow.name()
    );
    let gateway = Trusting::start(&fetched)?;
    let b = format!("Bearer {}", slow.mint(&Claims::new(slow.name(), AUDIENCE))?);

    let in_flight = tokio::spawn(gateway.app.clone().oneshot(described(&b)?));
    let started = std::time::Instant::now();
    while !keys
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .any(|request| request.url.path() == "/jwks")
    {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the key set was never asked for"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    gateway.write_auth(&auth_a())?;
    let applied = gateway.reloader.reload()?;
    assert!(applied.auth_changed, "{applied:?}");
    assert!(
        !in_flight.is_finished(),
        "the request was still being admitted when the reload applied"
    );

    let status = in_flight.await??.status();
    assert_eq!(
        StatusCode::OK,
        status,
        "the request in flight is admitted by the issuers it took"
    );
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_of(&gateway.app, &b).await?,
        "a request after the reload no longer trusts the issuer"
    );
    Ok(())
}

#[tokio::test]
async fn an_auth_that_does_not_load_is_refused_with_the_whole_reload() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    gateway.write_registry(
        &(member("a", "http://127.0.0.1:9/a") + &member("b", "http://127.0.0.1:9/b")),
    )?;
    let unverifiable = format!(
        "{}\n[[auth.issuer]]\nissuer = \"http://issuer-b.example.test\"\n",
        auth_a()
    );
    gateway.write_auth(&unverifiable)?;

    let refused = gateway
        .reloader
        .reload()
        .err()
        .ok_or("an issuer whose metadata would travel in the clear is refused")?;
    assert_eq!("configuration", refused.class(), "{refused:?}");
    assert_eq!(1, gateway.members()?, "the registry change is not applied");
    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &support::bearer()?).await?,
        "the running issuers stay"
    );
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status_of(&gateway.app, &bearer_b()?).await?
    );
    Ok(())
}

#[tokio::test]
async fn an_auth_that_trusts_no_issuer_is_refused_and_the_running_issuers_stay() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    gateway.write_auth("")?;

    let refused = gateway
        .reloader
        .reload()
        .err()
        .ok_or("a federating gateway that trusts no issuer is refused")?;
    assert_eq!("auth", refused.class(), "{refused:?}");
    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &support::bearer()?).await?,
        "the running issuers stay"
    );
    Ok(())
}

#[tokio::test]
async fn a_changed_audience_waits_for_a_restart_while_the_issuers_apply() -> TestResult {
    let gateway = Trusting::start(&auth_a())?;
    let moved = auth_a_and_b()?.replace(AUDIENCE, "urn:example:another-audience");
    gateway.write_auth(&moved)?;

    let applied = gateway.reloader.reload()?;
    assert_eq!(vec!["auth.audience"], applied.needs_restart);
    assert_eq!(
        StatusCode::OK,
        status_of(&gateway.app, &bearer_b()?).await?,
        "the added issuer applies, under the audience the process started with"
    );
    assert!(gateway.trust.gate().admits_any());
    Ok(())
}
