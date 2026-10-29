// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a capturing log writer, test settings, a test state, the
//! typed shapes the tests read the server's JSON with, a mock node's routes
//! and what it was asked, the reads of a routed answer, and the dependency
//! report. A conveyed token as a node reads it is in [`conveyed`], and the
//! addresses a `serve` process bound in [`serving`].

pub(crate) mod conveyed;
pub(crate) mod serving;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::auth::{AuthSettings, IssuerSettings, Verification};
use ferrofed_server::config::settings::ServerSettings;
use ferrofed_server::error::{CODE_MEMBER, REQUEST_ID_MEMBER};
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{Claims, Issuer, IssuerError};
use ferrofed_testkit::mock::Server;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use http::{HeaderMap, Request, Response, StatusCode, header};
use openehr_federation::headers::{ENDPOINT, SYSTEM_ID};
use openehr_its::rest::generated::common::Error;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::io::{self, Write};
use std::num::TryFromIntError;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Duration;
use tower::ServiceExt as _;
use tracing_subscriber::fmt::MakeWriter;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

/// The time a loaded host may add to any wait a test makes.
///
/// A bound on elapsed time sits this far past what the code under test
/// should take, and a node meant to be abandoned stays silent at least this
/// far past the bound, so neither side of the claim depends on how busy the
/// machine is.
pub(crate) const SLACK: Duration = Duration::from_secs(3);

/// Returns `duration` in whole milliseconds, as the configuration spells it.
pub(crate) fn millis(duration: Duration) -> Result<u64, TryFromIntError> {
    u64::try_from(duration.as_millis())
}

/// Returns `error` and every cause behind it, as one line.
pub(crate) fn chain(error: &(dyn StdError + 'static)) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

/// A `tracing` writer that keeps every line in memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Returns everything written so far.
    pub(crate) fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

/// The writer [`Logs`] hands out.
#[derive(Debug)]
pub(crate) struct LogsWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = LogsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogsWriter(Arc::clone(&self.0))
    }
}

/// One JSON log line, read for the fields the tests assert on.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct LogLine {
    /// The event message.
    pub(crate) message: String,
    /// The HTTP method of a request line.
    pub(crate) method: Option<String>,
    /// The matched route of a request line.
    pub(crate) route: Option<String>,
    /// The status of a request line.
    pub(crate) status: Option<u16>,
    /// The latency of a request line.
    pub(crate) latency_ms: Option<f64>,
    /// The logged query pairs of a request line.
    pub(crate) query: Option<String>,
    /// The request id of a request line: the gateway's outbound id.
    pub(crate) request_id: Option<String>,
    /// Whether the client named its request, on a request line.
    pub(crate) client_named: Option<bool>,
    /// Where a thread panicked, on the panic hook's line.
    pub(crate) location: Option<String>,
    /// The address a listener bound, on its start line.
    pub(crate) listen: Option<String>,
}

/// Returns every JSON line in `text`, in order.
pub(crate) fn lines(text: &str) -> Result<Vec<LogLine>, serde_json::Error> {
    text.lines().map(serde_json::from_str).collect()
}

/// Returns the request lines in `text`.
pub(crate) fn request_lines(text: &str) -> Result<Vec<LogLine>, serde_json::Error> {
    Ok(lines(text)?
        .into_iter()
        .filter(|line| line.message == "request")
        .collect())
}

/// The issuer identifier of the suite's test issuer.
pub(crate) const ISSUER: &str = "https://issuer.example.test";

/// The audience the gateway under test is known by at the test issuer.
pub(crate) const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// The suite's test issuer, its key generated once per test process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static TEST_ISSUER: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(ISSUER).expect("a test key pair should generate"));

/// Returns the suite's test issuer, which every test gateway trusts.
pub(crate) fn issuer() -> &'static Issuer {
    &TEST_ISSUER
}

/// Returns the claims of a token the test gateway admits for every
/// operation: every scope and the purpose of use `TREAT`.
pub(crate) fn claims() -> Claims {
    Claims::new(ISSUER, AUDIENCE)
}

/// Returns a fresh token with the default [`claims`].
pub(crate) fn token() -> Result<String, IssuerError> {
    issuer().mint(&claims())
}

/// A default token minted once per test process: the client credential a
/// test sends explicitly and then searches for at the node, where it must
/// never arrive (§13.1).
#[expect(
    clippy::expect_used,
    reason = "a test process whose issuer cannot sign cannot test anything"
)]
pub(crate) static CLIENT_TOKEN: LazyLock<String> =
    LazyLock::new(|| token().expect("the test issuer should sign"));

/// Returns the `Authorization` value carrying a fresh default [`token`].
pub(crate) fn bearer() -> Result<String, IssuerError> {
    Ok(format!("Bearer {}", token()?))
}

/// Returns the `Authorization` value of another verified caller: the default
/// [`claims`] with `subject` as `sub`.
pub(crate) fn bearer_as(subject: &str) -> Result<String, IssuerError> {
    let mut claims = claims();
    subject.clone_into(&mut claims.sub);
    Ok(format!("Bearer {}", issuer().mint(&claims)?))
}

/// The product scope, `[auth] product_scope`, which admits operators and
/// which no default token carries; the SMART extension form of a string
/// starting with `__` (SMART App Launch 2.2.0, Scopes and Launch Context,
/// Extensibility).
pub(crate) const OPERATOR_SCOPE: &str = "__ferrofed.operator";

/// Returns the `Authorization` value of an operator: the default [`claims`]
/// with [`OPERATOR_SCOPE`] added to the scope.
pub(crate) fn operator_bearer() -> Result<String, IssuerError> {
    bearer_adding_scope(OPERATOR_SCOPE)
}

/// Returns the `Authorization` value of the default [`claims`] with `extra`
/// added to the scope as one more token.
pub(crate) fn bearer_adding_scope(extra: &str) -> Result<String, IssuerError> {
    let mut claims = claims();
    let scope = claims.scope.take().unwrap_or_default();
    claims.scope = Some(format!("{scope} {extra}").trim().to_owned());
    Ok(format!("Bearer {}", issuer().mint(&claims)?))
}

/// The file the suite's test issuer's key set is written to, once per test
/// process, under the target's temporary directory.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot write its key set file cannot test anything"
)]
static JWKS_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    let file = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("ferrofed-it-jwks-{}.json", std::process::id()));
    issuer()
        .write_jwks(&file)
        .expect("the key set file should be written");
    file
});

/// Returns the file the suite's test issuer's key set is written to.
pub(crate) fn jwks_file() -> &'static Path {
    &JWKS_FILE
}

/// Returns a file under the target's temporary directory holding
/// `issuer`'s key set as it is now, kept for the life of the test process.
pub(crate) fn jwks_file_of(issuer: &Issuer) -> Result<PathBuf, Box<dyn StdError>> {
    let file = tempfile::NamedTempFile::new_in(env!("CARGO_TARGET_TMPDIR"))?
        .into_temp_path()
        .keep()?;
    issuer.write_jwks(&file)?;
    Ok(file)
}

/// Returns how `issuer`'s tokens addressed to [`AUDIENCE`] are verified:
/// against the key set in `jwks_file`, under `ferrohealth-smart`'s defaults.
pub(crate) fn key_set(issuer: &str, jwks_file: &Path) -> Verification {
    let mut settings = KeySetSettings::new(issuer, AUDIENCE);
    settings.jwks_file = Some(jwks_file.to_owned());
    Verification::KeySet(Box::new(settings))
}

/// Returns the `[auth]` of every test gateway: the test issuer is trusted,
/// its key set read from [`jwks_file`], the default client is a demographic
/// client, and [`OPERATOR_SCOPE`] admits operators.
pub(crate) fn auth() -> AuthSettings {
    let mut trusted = IssuerSettings::new(ISSUER, key_set(ISSUER, jwks_file()));
    trusted.demographic_clients = BTreeSet::from([claims().client_id]);
    AuthSettings {
        audience: vec![AUDIENCE.to_owned()],
        product_scope: Some(OPERATOR_SCOPE.to_owned()),
        issuers: vec![trusted],
        ..AuthSettings::default()
    }
}

/// Returns the `[auth]` and `[trust]` of [`auth`] as configuration text,
/// its issuer an `[[auth.issuer]]` entry, so a test can append another.
pub(crate) fn auth_toml() -> String {
    format!(
        "\n[auth]\naudience = [\"{AUDIENCE}\"]\nproduct_scope = \"{OPERATOR_SCOPE}\"\n\n[[auth.issuer]]\nissuer = \"{ISSUER}\"\njwks_file = {}\n\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\ndemographic_clients = [\"{}\"]\n",
        toml_path(jwks_file()),
        claims().client_id
    )
}

/// Returns the `[[auth.issuer]]` entry of `issuer`, its key set written to
/// a file as it is now.
pub(crate) fn issuer_toml(issuer: &Issuer) -> Result<String, Box<dyn StdError>> {
    Ok(format!(
        "\n[[auth.issuer]]\nissuer = \"{}\"\njwks_file = {}\n",
        issuer.name(),
        toml_path(&jwks_file_of(issuer)?)
    ))
}

/// The Basic user a scrape of the admin listener authenticates as.
pub(crate) const SCRAPE_USER: &str = "synthetic-scraper";

/// The synthetic password of [`SCRAPE_USER`].
pub(crate) const SCRAPE_PASSWORD: &str = "synthetic-scrape-password";

/// The Argon2id PHC string of [`SCRAPE_PASSWORD`], under a fixed synthetic
/// salt.
pub(crate) const SCRAPE_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$4es9c3W3V1fKeaFNam0B7dB/9kjZXP6/+9VB3yhq8ZA";

/// Returns the `Authorization` value of `user` with `password` in the
/// Basic scheme (RFC 7617 §2).
pub(crate) fn basic(user: &str, password: &str) -> String {
    use base64::Engine as _;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
}

/// Returns the `[auth]` of [`auth`] with `users`, each holding
/// [`SCRAPE_PASSWORD`], as its `[[auth.basic.users]]`.
pub(crate) fn auth_with_basic(users: &[&str]) -> Result<AuthSettings, Box<dyn StdError>> {
    use ferrohealth_smart::basic::PasswordStore;
    use ferrohealth_smart::config::{BasicSection, BasicUser};
    use secrecy::SecretString;
    let mut store = PasswordStore::new();
    let mut written = BasicSection::default();
    for user in users {
        store = store.with_user(*user, SecretString::from(SCRAPE_HASH), "")?;
        written.users.push(BasicUser {
            user: (*user).to_owned(),
            password_hash: Some(SecretString::from(SCRAPE_HASH)),
            ..BasicUser::default()
        });
    }
    Ok(AuthSettings {
        basic: Some(ferrofed_server::config::auth::BasicUsers::new(
            store, written,
        )),
        ..auth()
    })
}

/// The `[metrics]` key naming [`SCRAPE_USER`] as the scrape's user, then
/// the `[auth]` that writes that user with HTTP Basic.
pub(crate) fn scraping() -> String {
    format!(
        "scrape_users = [\"{SCRAPE_USER}\"]\n\n[auth]\n\n[[auth.basic.users]]\nuser = \"{SCRAPE_USER}\"\npassword_hash = \"{SCRAPE_HASH}\"\n"
    )
}

/// Returns `path` as a TOML string.
pub(crate) fn toml_path(path: &Path) -> toml::Value {
    toml::Value::String(path.display().to_string())
}

/// Returns server settings a test drives the middleware with.
#[expect(
    clippy::expect_used,
    reason = "the default [server] limits are positive, so they resolve"
)]
pub(crate) fn settings() -> ServerSettings {
    ServerSettings {
        listen: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        base_path: ferrofed_server::base_path::BasePath::default(),
        public_url: None,
        forwarding: ferrofed_server::client_address::Forwarding::default(),
        request_timeout: Duration::from_secs(5),
        header_read_timeout_ms: std::num::NonZeroU64::MIN.saturating_add(29_999),
        drain_delay: Duration::ZERO,
        shutdown_timeout: Duration::from_secs(5),
        bindings_drain: Duration::from_secs(5),
        body_limit: 1024,
        auth: auth(),
        overload: ferrofed_server::config::server::Server::default()
            .resolve_overload()
            .expect("the default limits should resolve"),
        tls: None,
        replica: ferrofed_registry::replica::Replica::default(),
    }
}

/// Returns a state with no indicator.
pub(crate) fn state() -> Arc<AppState> {
    Arc::new(AppState::default())
}

/// The application under test, with no indicator registered.
pub(crate) fn app() -> Router {
    ferrofed_server::router(state(), &settings())
}

/// Sends `request` through `app` and returns the whole response.
///
/// A request that carries no `Authorization` field is sent with a fresh
/// default [`token`], so every test reaches past client authentication
/// unless it sets the field itself.
pub(crate) async fn send(
    app: Router,
    mut request: Request<Body>,
) -> Result<Response<Body>, Box<dyn StdError>> {
    if !request.headers().contains_key(header::AUTHORIZATION) {
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, bearer()?.parse()?);
    }
    send_as_is(app, request).await
}

/// Returns an HTTP client that sends a fresh default [`token`] with every
/// request that carries no `Authorization` field of its own, for the tests
/// that drive a real socket.
pub(crate) fn authenticated_client() -> Result<reqwest::Client, Box<dyn StdError>> {
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, bearer()?.parse()?);
    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .build()?)
}

/// Sends `request` through `app` as it is, with no credential added.
pub(crate) async fn send_as_is(
    app: Router,
    request: Request<Body>,
) -> Result<Response<Body>, Box<dyn StdError>> {
    Ok(app.oneshot(request).await?)
}

/// Sends `request` through `app` and reads the status and the body.
pub(crate) async fn call(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn StdError>> {
    let response = send(app, request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, String::from_utf8(bytes.to_vec())?))
}

/// What the tests read of an error body.
#[derive(Debug)]
pub(crate) struct ErrorBody {
    /// The ITS-REST message.
    pub(crate) message: String,
    /// The ITS-REST validation errors.
    pub(crate) validation_errors: Vec<String>,
    /// The stable error code.
    pub(crate) code: String,
    /// The request id.
    pub(crate) request_id: String,
}

/// Reads `text` as the generated ITS-REST `Error`, which must carry the
/// stable code and the request id as its only extra members.
pub(crate) fn error_body(text: &str) -> Result<ErrorBody, Box<dyn StdError>> {
    let error: Error = serde_json::from_str(text)?;
    let member = |name: &str| -> Result<String, Box<dyn StdError>> {
        Ok(error
            .additional_properties
            .get(name)
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("the error body carries a string {name}: {text}"))?
            .to_owned())
    };
    let code = member(CODE_MEMBER)?;
    let request_id = member(REQUEST_ID_MEMBER)?;
    if error.additional_properties.len() != 2 {
        return Err(format!("the error body carries only code and request_id: {text}").into());
    }
    Ok(ErrorBody {
        message: error.message,
        validation_errors: error.validation_errors,
        code,
        request_id,
    })
}

/// The status, the headers and the body bytes `app` answers `request` with.
pub(crate) async fn exchange(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), Box<dyn StdError>> {
    let response = send(app, request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, bytes.to_vec()))
}

/// A node answering `verb` at `at` with `answer`, and `404` to the rest.
pub(crate) async fn mount(server: &Server, verb: &str, at: String, answer: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(server)
        .await;
}

/// The method and path of every request `server` received, in order.
pub(crate) async fn asked(server: &Server) -> Result<Vec<(String, String)>, Box<dyn StdError>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect())
}

/// The value of the field `name` in `headers`, when it is text.
pub(crate) fn field<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

/// Asserts that `headers` name `endpoint` and its node's `system_id` as the
/// ones that acted (§7a.3, N31, §9.6).
pub(crate) fn acted(headers: &HeaderMap, endpoint: &str, system_id: &str) {
    assert_eq!(Some(endpoint), field(headers, ENDPOINT), "N31");
    assert_eq!(Some(system_id), field(headers, SYSTEM_ID), "§9.6");
}

/// Asserts that `request` is refused `400` with `code`, names no acting
/// endpoint, and that neither node received anything.
pub(crate) async fn refused_at_neither(
    app: Router,
    request: Request<Body>,
    code: &str,
    (a, b): (&Server, &Server),
) -> Result<(), Box<dyn StdError>> {
    let (status, headers, body) = exchange(app, request).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(code, error_body(&text)?.code, "{text}");
    assert_eq!(None, field(&headers, ENDPOINT), "no endpoint acted: {text}");
    assert!(asked(a).await?.is_empty(), "node A received nothing");
    assert!(asked(b).await?.is_empty(), "node B received nothing");
    Ok(())
}

/// `GET /operator/dependencies` as an operator of the test issuer asks it.
pub(crate) fn dependencies_request() -> Result<Request<Body>, Box<dyn StdError>> {
    Ok(Request::get("/operator/dependencies")
        .header(header::AUTHORIZATION, operator_bearer()?)
        .body(Body::empty())?)
}

/// The state `GET /operator/dependencies` of `app` reports of each member
/// endpoint, by endpoint id.
pub(crate) async fn observed(app: &Router) -> Result<BTreeMap<String, String>, Box<dyn StdError>> {
    #[derive(Deserialize)]
    struct Report {
        endpoints: BTreeMap<String, String>,
    }
    let request = dependencies_request()?;
    let (status, text) = call(app.clone(), request).await?;
    if status != StatusCode::OK {
        return Err(format!("the dependency report answered {status}: {text}").into());
    }
    Ok(serde_json::from_str::<Report>(&text)?.endpoints)
}

/// The `(endpoint, state)` pairs `pairs` as [`observed`] returns them.
pub(crate) fn states(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(endpoint, state)| ((*endpoint).to_owned(), (*state).to_owned()))
        .collect()
}

/// What a search reads in place of a request id the gateway minted.
pub(crate) const MINTED_REQUEST_ID: &str = "<minted-request-id>";

/// Whether `text` has the form the gateway mints a request id in: a version
/// 4 UUID, hyphenated and lowercase, as `OutboundId` writes it.
pub(crate) fn is_minted_form(text: &str) -> bool {
    uuid::Uuid::try_parse(text).is_ok_and(|id| {
        id.get_version() == Some(uuid::Version::Random) && id.hyphenated().to_string() == text
    })
}

/// Returns `text` with each of `ids`, request ids the gateway minted, replaced
/// by [`MINTED_REQUEST_ID`].
///
/// A search for a short hexadecimal fragment reads the result, so a random
/// UUID cannot match it by chance.
///
/// # Errors
///
/// When an id is not in the minted form, so no other value is ever masked.
pub(crate) fn without_minted<'a>(
    text: &str,
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<String, Box<dyn StdError>> {
    let mut masked = text.to_owned();
    for id in ids {
        if !is_minted_form(id) {
            return Err(format!("{id:?} is not a request id the gateway minted").into());
        }
        masked = masked.replace(id, MINTED_REQUEST_ID);
    }
    Ok(masked)
}

/// The address every test gateway declares its JWK Set at.
pub(crate) const JWKS_URI: &str = "https://gw.example.org/.well-known/jwks.json";

/// The gateway signing key every test gateway that federates is configured
/// with: a synthetic ES384 key written once per test process, in a directory
/// that lives as long as the process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot write a synthetic key cannot test anything"
)]
static SIGNING_KEY: LazyLock<(tempfile::TempDir, String)> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("a temporary directory should be made");
    let file = dir.path().join("signing-key.pem");
    let pem = ferrofed_testkit::oauth::es384_pem().expect("a test key should generate");
    std::fs::write(&file, pem).expect("the key file should be written");
    (dir, file.display().to_string())
});

/// The `[signing]` table every test gateway that federates carries, so it
/// can sign the caller's identity for each node (§13.1, N24).
pub(crate) fn signing_toml() -> String {
    format!(
        "\n[signing]\nkey_file = {}\njwks_uri = \"{JWKS_URI}\"\n",
        toml::Value::String(SIGNING_KEY.1.clone())
    )
}

/// The file the suite's gateway signing key is written to.
pub(crate) fn signing_key_file() -> &'static str {
    &SIGNING_KEY.1
}

/// `text` with the [`signing_toml`] table appended when it configures a
/// registry and no `[signing]` of its own, and with an `[audit]` table when
/// it configures a registry and no `[audit]` of its own (Regulation (EU)
/// 2025/327 Annex II 3.2): [`DEVELOPMENT_AUDIT_TOML`] under the development
/// profile, and the [`audit_toml`] tables under any other.
///
/// The `[testkit]` rows `text` writes become the test kit's fixed
/// cross-reference, injected through the server's `test-seam` point and
/// removed from the text (#1377).
pub(crate) fn signed(text: &str) -> String {
    sign(&injected(text))
}

/// [`signed`] for a gateway in a child process, which no injection reaches:
/// the `[testkit]` rows are dropped, so a child that has to resolve a
/// patient is configured with a PIX Manager of its own.
pub(crate) fn spawned(text: &str) -> String {
    sign(&ferrofed_testkit::crossref::take(text).0)
}

/// A `[[pixm.manager]]` that resolves every node of the registry document
/// `registry` and is never asked, for a spawned `config check`, which needs
/// a resolver configured and resolves no patient.
///
/// # Panics
///
/// When `registry` is no registry document, which fails the test.
#[cfg(feature = "binding-ihe")]
#[expect(
    clippy::expect_used,
    reason = "a test helper over the suite's own registry documents"
)]
pub(crate) fn unasked_pixm(registry: &str) -> String {
    use std::fmt::Write as _;

    let snapshot = ferrofed_registry::snapshot::RegistrySnapshot::from_toml_str(registry)
        .expect("the test's registry document loads");
    let members = snapshot
        .nodes()
        .enumerate()
        .fold(String::new(), |mut members, (index, node)| {
            // NOTE: writing to a String cannot fail, so the result is dropped.
            let _written: std::fmt::Result = writeln!(
                members,
                "\"{}\" = \"urn:oid:2.999.2.{}\"",
                node.id(),
                index + 1
            );
            members
        });
    format!(
        "\n[[pixm.manager]]\nurl = \"http://127.0.0.1:9/fhir/\"\n\n[pixm.manager.members]\n{members}"
    )
}

/// `text` without its `[testkit]` table, whose rows are injected as the test
/// kit's fixed cross-reference, or with the injection cleared when it has
/// none; nothing else is added, for a test that writes its own `[signing]`
/// and `[audit]`.
pub(crate) fn injected(text: &str) -> String {
    use ferrofed_identity::role::consent::ConsentPrefilter;
    use ferrofed_identity::role::localizer::Localizer;
    use ferrofed_server::binding::injected::{Injection, PrefilterFactory, inject};
    use ferrofed_server::binding::seam::ResolverSeam;
    use ferrofed_testkit::crossref::{Fixture, MODE, take};

    let (text, fixture) = take(text);
    inject(fixture.map(|fixture: Fixture| {
        let consent = fixture.clone();
        let prefilter = fixture.denies().then(|| -> PrefilterFactory {
            Arc::new(move |snapshot| {
                let prefilter: Arc<dyn ConsentPrefilter> = consent.prefilter(snapshot)?;
                Ok(prefilter)
            })
        });
        Injection {
            resolver: Arc::new(move |snapshot| {
                let resolver = fixture.resolver(snapshot)?;
                let localizer: Arc<dyn Localizer> = resolver.clone();
                Ok(ResolverSeam {
                    resolver,
                    localizer: Some((localizer, MODE)),
                })
            }),
            prefilter,
        }
    }));
    text
}

/// `text` with the tables of [`signed`] appended, as written otherwise.
fn sign(text: &str) -> String {
    let mut text = text.to_owned();
    if text.contains("[registry") && !text.contains("[signing]") {
        text.push_str(&signing_toml());
    }
    if text.contains("[registry") && !text.contains("[audit") {
        if text.contains("profile = \"development\"") {
            text.push_str(DEVELOPMENT_AUDIT_TOML);
        } else {
            text.push_str(&audit_toml());
        }
    }
    text
}

/// The `[audit]` table a development test gateway that federates carries
/// unless it names its own: the records go to the log target, which the
/// development profile alone accepts for the access log.
pub(crate) const DEVELOPMENT_AUDIT_TOML: &str = "\n[audit]\ndestination = \"log\"\n";

/// `text` with the [`signing_toml`] table appended when it configures a
/// registry and no `[signing]` of its own, and no `[audit]` added: for a
/// test of what an absent `[audit]` means.
#[cfg_attr(
    not(feature = "binding-ihe"),
    expect(
        dead_code,
        reason = "only the IHE binding's tests read an absent [audit]"
    )
)]
pub(crate) fn signing_only(text: &str) -> String {
    if text.contains("[registry") && !text.contains("[signing]") {
        format!("{text}{}", signing_toml())
    } else {
        text.to_owned()
    }
}

/// The spool directory of [`audit_toml`]: one per test process, in a
/// directory that lives as long as the process, so every gateway of a test
/// shares one forwarder.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot make a temporary directory cannot test anything"
)]
static AUDIT_SPOOL: LazyLock<(tempfile::TempDir, String)> = LazyLock::new(|| {
    let dir = tempfile::tempdir().expect("a temporary directory should be made");
    let spool = dir.path().join("audit-feed-spool").display().to_string();
    (dir, spool)
});

/// The `[audit]` tables a test gateway that federates carries unless it
/// names its own. Outside development a registry needs a repository for its
/// access records, since `log` names no caller and no patient (Regulation
/// (EU) 2025/327 Annex II 3.2). The repository is an `https` base no
/// connection reaches, because the testkit's harness repository speaks plain
/// `http`, which production refuses: each record is stored in the spool on
/// disk, and so recorded, and its delivery is retried. A test of what the
/// repository receives names its own `[audit]` over the harness.
pub(crate) fn audit_toml() -> String {
    format!(
        "\n[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"https://{}/arr/\"\nhostname = \"gateway.example.org\"\nspool_dir = {}\n",
        ferrofed_testkit::unreachable::ADDRESS,
        toml::Value::String(AUDIT_SPOOL.1.clone())
    )
}

/// A signer over a fresh synthetic key, naming the gateway `federation`,
/// for a test that assembles its own federation.
pub(crate) fn signer(
    federation: &str,
) -> Result<Arc<ferrofed_engine::conveyance::signer::Signer>, Box<dyn StdError>> {
    use ferrofed_engine::onward::SystemClock;
    use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
    let pem = secrecy::SecretString::from(ferrofed_testkit::oauth::es384_pem()?);
    let keys = KeyRing::new(
        SigningKey::from_pem(&pem)?,
        None,
        Duration::ZERO,
        Arc::new(SystemClock),
    )?;
    Ok(Arc::new(ferrofed_engine::conveyance::signer::Signer::new(
        Arc::new(keys),
        federation,
    )))
}

/// A conveyance of a synthetic verified caller, for a test that dispatches
/// through the engine itself.
pub(crate) fn conveyance() -> Result<ferrofed_engine::conveyance::Conveyance, Box<dyn StdError>> {
    use ferrofed_engine::conveyance::Conveyance;
    use ferrofed_engine::conveyance::caller::{Acting, Caller, Principal, Verification};
    let caller = Caller {
        issuer: ISSUER.to_owned(),
        subject: "clinician-0042".to_owned(),
        organisation: None,
        purposes: Vec::new(),
        scope: String::new(),
        verified_by: Verification::Signature,
        professional: Box::default(),
        acting: Acting::Person,
        assurance_level: None,
        relayed: None,
    };
    Ok(Conveyance::new(
        signer("example-federation")?,
        Principal::Caller(caller),
    ))
}

/// Serves `app` on `listener` as `ferrofed serve` serves its listeners
/// (`ferrohealth_server::serve`), until `signal` completes, then gives the
/// requests in flight `drain` to finish.
///
/// # Errors
///
/// Never fails today, as the server's own `serve`.
pub(crate) async fn serve_until<F>(
    listener: tokio::net::TcpListener,
    app: Router,
    drain: Duration,
    signal: F,
) -> io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    ferrohealth_server::serve::serve(
        listener,
        app,
        ferrohealth_health::lifecycle::Lifecycle::default(),
        signal,
        Duration::ZERO,
        drain,
    )
    .await
    .map(|_drained| ())
}
