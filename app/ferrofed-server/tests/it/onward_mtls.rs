// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Mutual TLS on the onward grant through the real configuration path: a
//! node's `[credentials]` section names the client identity and the trust
//! roots, the gateway reaches the node and its authorization server over a
//! transport that presents that certificate, authenticates with
//! `tls_client_auth` and takes tokens bound to the certificate, and the
//! configuration refuses what cannot work (§13.1, §13.4, N25, CP-17; RFC
//! 8705 §2, §3).
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
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::{Scheme, Settings};
use ferrofed_server::config::tls::TlsFault;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_server::onward::OnwardError;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuance::Verdict;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use ferrofed_testkit::tls::MutualTls;
use ferrofed_testkit::unreachable;
use ferrohealth_server::tls_client::TlsClientError;
use ferrohealth_smart::client::ClientAuth;
use ferrohealth_smart::client::fapi2::{AliasHost, SecurityProfile};
use ferrohealth_smart::config::ConfigError as SmartError;
use ferrohealth_smart::sender::ClientCertificate;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{PATIENT, body, crossref, patient_query, post, registry, settings_with_room};
use crate::support::caller::CLIENT_TOKEN;
use crate::support::{call, injected};

type TestResult = Result<(), Box<dyn Error>>;

/// The client node A's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope the gateway requests onward.
const SCOPE: &str = "system/aql-*.s";

/// The JWK Set location the gateway declares.
const JWKS_URI: &str = "https://gw.example.org/.well-known/jwks.json";

/// A one-row answer from a node.
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// Node A and its authorization server on one origin, behind one
/// mutual-TLS front.
struct NodeA {
    endpoint: TokenEndpoint,
    front: MutualTls,
}

impl NodeA {
    /// The authorization server, authenticating by certificate, and the node
    /// beside it, answering a token the server issued, or any request at
    /// all when `open`.
    async fn start(open: bool) -> Result<Self, Box<dyn Error>> {
        let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
        endpoint.accept_tls_client_auth();
        endpoint.expect_scope(SCOPE);
        let answering = Mock::given(method("POST")).and(path("/v1/query/aql"));
        let answering = if open {
            answering
        } else {
            answering.and(endpoint.bearer())
        };
        answering
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(ONE_ROW.as_bytes().to_vec(), "application/json"),
            )
            .with_priority(1)
            .mount(endpoint.server())
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/query/aql"))
            .respond_with(ResponseTemplate::new(401))
            .with_priority(2)
            .mount(endpoint.server())
            .await;
        let front = MutualTls::front(&endpoint.server().uri())?;
        Ok(Self { endpoint, front })
    }

    /// The node's base URL behind the front.
    fn base(&self) -> String {
        self.front.origin()
    }

    /// The token endpoint behind the front.
    fn token_url(&self) -> String {
        format!("{}{}", self.front.origin(), oauth::TOKEN_PATH)
    }

    /// The requests the node was sent.
    async fn node_requests(&self) -> Result<Vec<wiremock::Request>, Box<dyn Error>> {
        Ok(self
            .endpoint
            .server()
            .received_requests()
            .await
            .ok_or("recording is on")?
            .into_iter()
            .filter(|request| request.url.path() == "/v1/query/aql")
            .collect())
    }
}

/// The `[signing]` table with a fresh ES384 key, written into `dir`.
fn signing(dir: &Path) -> Result<String, Box<dyn Error>> {
    let file = dir.join("signing.pem");
    std::fs::write(&file, oauth::Curve::P384.pem()?)?;
    Ok(format!(
        "[signing]\nkey_file = {}\njwks_uri = \"{JWKS_URI}\"\n",
        toml::Value::String(file.display().to_string())
    ))
}

/// The TLS keys of node A's section, presenting `front`'s client identity
/// and trusting it, the files written into `dir`.
fn tls_keys(dir: &Path, front: &MutualTls) -> Result<String, Box<dyn Error>> {
    let identity = dir.join("node-a-client.pem");
    let roots = dir.join("node-a-roots.pem");
    std::fs::write(&identity, front.client_identity())?;
    std::fs::write(&roots, front.trust_roots())?;
    Ok(format!(
        "tls = {{ client_identity_file = {}, trust_roots_file = {} }}\n",
        toml::Value::String(identity.display().to_string()),
        toml::Value::String(roots.display().to_string())
    ))
}

/// Node A's `[credentials]` section: `tls` and an `oauth` grant at
/// `token_url` authenticating by certificate, with `extra` keys.
fn credentials(tls: &str, token_url: &str, extra: &str) -> String {
    format!(
        "[credentials.\"node-a-pub\"]\n{tls}\n[credentials.\"node-a-pub\".oauth]\nclient_auth = \"tls_client_auth\"\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nscope = \"{SCOPE}\"\n{extra}"
    )
}

/// Whether `error` is the crate's refusal of node A's `oauth` table that
/// `expected` admits.
fn smart(error: &ConfigError, expected: impl Fn(&SmartError) -> bool) -> bool {
    matches!(error, ConfigError::Smart { section, source }
        if section == "credentials.node-a-pub.oauth" && expected(source))
}

/// The configuration text of a gateway over node A at `a` and node B at
/// `b`, with `tables` after the federation, its registry written into `dir`.
fn text(dir: &Path, (a, b): (&str, &str), tables: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout = '2s'\noverall_timeout = '3s'\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n{}\n{}\n{tables}",
        crossref(&[("node-a", "2222aaaa-2222-4222-8222-222222222222")]),
        signing(dir)?
    ))
}

/// The settings `text` resolves to.
fn settings(text: &str) -> Result<Settings, Box<dyn Error>> {
    Ok(Config::from_sources(Some(&injected(text)), &BTreeMap::new())?.resolve()?)
}

/// The gateway the settings of `text` build.
fn gateway(text: &str) -> Result<Router, Box<dyn Error>> {
    let settings = settings(text)?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    Ok(ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    ))
}

/// The configuration error `text` resolves to.
fn refused(text: &str) -> Result<ConfigError, Box<dyn Error>> {
    match Config::from_sources(Some(&injected(text)), &BTreeMap::new())?.resolve() {
        Ok(_) => Err(format!("accepted: {text}").into()),
        Err(error) => Ok(error),
    }
}

/// The patient query, carrying the caller's own `Authorization`.
fn patient_post() -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {}", *CLIENT_TOKEN).parse()?,
    );
    Ok(request)
}

/// The gateway authenticates to node A's authorization server by the
/// certificate its section names, sends `client_id` and no assertion, takes
/// a token bound to that certificate, and the node receives it over a
/// connection presenting the same certificate; neither the caller's token
/// nor the patient reaches either (RFC 8705 §2, §3; N25, N33).
// conformance: CP-17
#[tokio::test]
async fn a_node_is_reached_with_a_token_bound_to_the_certificate_its_section_names() -> TestResult {
    let a = NodeA::start(false).await?;
    a.endpoint.bind_to_certificate(a.front.client_thumbprint());
    let dir = tempfile::tempdir()?;
    let tables = credentials(
        &tls_keys(dir.path(), &a.front)?,
        &a.token_url(),
        "certificate_bound = true\n",
    );
    let app = gateway(&text(dir.path(), (&a.base(), unreachable::BASE), &tables)?)?;

    let (status, text) = call(app, patient_post()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(vec![Verdict::Issued], a.endpoint.issuance().verdicts());
    let forms = a.endpoint.issuance().forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    // NOTE: RFC 6749 Appendix B, a form's parameters carry no order, so the
    // request is compared as the set of names it sends, each once.
    let mut names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(vec!["client_id", "grant_type", "scope"], names);
    for (name, value) in form {
        assert!(!value.contains(CLIENT_TOKEN.as_str()), "{name}");
        assert!(!value.contains(PATIENT), "{name} carries the patient (N33)");
    }
    let requests = a.node_requests().await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    let sent = request
        .headers
        .get(header::AUTHORIZATION)
        .ok_or("the node receives a credential")?
        .to_str()?;
    assert!(sent.starts_with("Bearer "), "{sent}");
    assert!(!sent.contains(CLIENT_TOKEN.as_str()));
    let presented = a.front.presented();
    assert_ne!(presented, Vec::<String>::new());
    assert!(
        presented
            .iter()
            .all(|thumbprint| thumbprint == a.front.client_thumbprint()),
        "{presented:?}"
    );
    Ok(())
}

/// A token bound to another certificate is refused before the node is sent
/// anything, and node A fails (RFC 8705 §3; §11.1).
// conformance: CP-17
#[tokio::test]
async fn a_token_bound_to_another_certificate_never_reaches_the_node() -> TestResult {
    let a = NodeA::start(false).await?;
    let other = ClientCertificate::from_der(b"a certificate the gateway does not hold");
    a.endpoint.bind_to_certificate(other.thumbprint());
    let dir = tempfile::tempdir()?;
    let tables = credentials(
        &tls_keys(dir.path(), &a.front)?,
        &a.token_url(),
        "certificate_bound = true\n",
    );
    let app = gateway(&text(dir.path(), (&a.base(), unreachable::BASE), &tables)?)?;

    let (status, text) = call(app, patient_post()?).await?;
    assert_ne!(StatusCode::OK, status, "{text}");
    assert_eq!(vec![Verdict::Issued], a.endpoint.issuance().verdicts());
    assert!(
        a.node_requests().await?.is_empty(),
        "never sent to the node"
    );
    assert!(!text.contains("eyJ"), "no token reaches the answer: {text}");
    Ok(())
}

/// A section that names TLS material alone reaches its node over mutual TLS
/// with no `Authorization`: the transport authenticates the gateway.
#[tokio::test]
async fn a_section_with_tls_material_alone_reaches_its_node_over_mutual_tls() -> TestResult {
    let a = NodeA::start(true).await?;
    let dir = tempfile::tempdir()?;
    let tables = format!(
        "[credentials.\"node-a-pub\"]\n{}",
        tls_keys(dir.path(), &a.front)?
    );
    let app = gateway(&text(dir.path(), (&a.base(), unreachable::BASE), &tables)?)?;

    let (status, text) = call(app, patient_post()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let requests = a.node_requests().await?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    assert!(request.headers.get(header::AUTHORIZATION).is_none());
    assert_eq!(
        vec![a.front.client_thumbprint().to_owned()],
        a.front.presented()
    );
    Ok(())
}

/// A grant that binds its tokens with `DPoP` and to the certificate at once
/// is refused at load: a token is bound one way (RFC 9449, RFC 8705 §3).
#[test]
fn dpop_and_certificate_binding_on_one_grant_are_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let identity = dir.path().join("client.pem");
    let dpop = dir.path().join("dpop.pem");
    std::fs::write(&dpop, oauth::Curve::P256.pem()?)?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    std::fs::write(&identity, front.client_identity())?;
    let tls = format!(
        "tls = {{ client_identity_file = {} }}\n",
        toml::Value::String(identity.display().to_string())
    );
    let both = format!(
        "certificate_bound = true\ndpop_key_file = {}\n",
        toml::Value::String(dpop.display().to_string())
    );
    let base = text(
        dir.path(),
        ("https://cdr-a.example.org/openehr", unreachable::BASE),
        "",
    )?;
    let error = refused(&format!(
        "{base}{}",
        credentials(&tls, "https://idp.example.org/token", &both)
    ))?;
    let two_bindings = |error: &ConfigError| {
        smart(error, |refused| {
            matches!(refused, SmartError::Exclusive { left, right }
                if *left == "credentials.oauth.dpop_key_file"
                    && *right == "credentials.oauth.certificate_bound")
        })
    };
    assert!(two_bindings(&error), "{error:?}");

    let fapi2 = format!(
        "{base}[credentials.\"node-a-pub\"]\n{tls}\n[credentials.\"node-a-pub\".oauth]\nissuer = \"https://as.cdr-a.example.org\"\nsecurity_profile = \"fapi2\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"tls_client_auth\"\nscope = \"{SCOPE}\"\n{both}"
    );
    let error = refused(&fapi2)?;
    assert!(two_bindings(&error), "{error:?}");
    Ok(())
}

/// Mutual TLS needs a certificate and TLS under every profile: a grant that
/// authenticates by, or binds to, a certificate its section does not name is
/// refused, so is one whose token endpoint is plain `http`, and so is an
/// identity with no certificate in it (RFC 8705 §2, §3.1).
#[test]
fn mutual_tls_without_a_certificate_or_over_http_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let base = text(
        dir.path(),
        ("https://cdr-a.example.org/openehr", unreachable::BASE),
        "",
    )?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tls = tls_keys(dir.path(), &front)?;

    for extra in ["", "certificate_bound = true\n"] {
        let error = refused(&format!(
            "{base}{}",
            credentials("", "https://idp.example.org/token", extra)
        ))?;
        assert!(
            matches!(
                &error,
                ConfigError::TlsFault(TlsFault::WithoutClientIdentity { .. })
            ),
            "{error:?}"
        );
    }

    let error = refused(&format!(
        "{base}{}",
        credentials(&tls, "http://idp.example.org/token", "")
    ))?;
    assert!(
        matches!(
            &error,
            ConfigError::TlsFault(TlsFault::Cleartext { key })
                if key == "credentials.node-a-pub.oauth.token_endpoint"
        ),
        "{error:?}"
    );

    let keyless = dir.path().join("keyless.pem");
    std::fs::write(&keyless, oauth::Curve::P256.pem()?)?;
    let error = refused(&format!(
        "{base}{}",
        credentials(
            &format!(
                "tls = {{ client_identity_file = {} }}\n",
                toml::Value::String(keyless.display().to_string())
            ),
            "https://idp.example.org/token",
            ""
        )
    ))?;
    assert!(
        matches!(
            &error,
            ConfigError::TlsClient(TlsClientError::Pem { key, .. })
                if key == "credentials.node-a-pub.tls.client_identity"
        ),
        "{error:?}"
    );
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("PRIVATE KEY"), "{shown}");
    Ok(())
}

/// A node presented the client certificate is reached over `https` alone,
/// under the development profile too.
#[test]
fn a_client_certificate_is_never_presented_over_http() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tables = format!(
        "[credentials.\"node-a-pub\"]\n{}",
        tls_keys(dir.path(), &front)?
    );
    let settings = settings(&text(
        dir.path(),
        ("http://127.0.0.1:9/a", unreachable::BASE),
        &tables,
    )?)?;
    let refused = Federation::load(&settings)
        .err()
        .ok_or("an http node presented a certificate was built")?;
    assert!(
        matches!(
            &refused,
            FederationError::Onward(OnwardError::ClientCertificateOverHttp { endpoint })
                if endpoint.as_str() == "node-a-pub"
        ),
        "{refused:?}"
    );
    Ok(())
}

/// A FAPI 2.0 grant that authenticates by, and binds its tokens to, the
/// certificate needs neither a client key nor a `DPoP` key (FAPI 2.0
/// Security Profile §5.3.2.1; RFC 8705 §2, §3).
#[test]
fn a_fapi2_grant_by_certificate_needs_no_key_of_its_own() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tables = format!(
        "[credentials.\"node-a-pub\"]\n{}\n[credentials.\"node-a-pub\".oauth]\nissuer = \"https://as.cdr-a.example.org\"\nsecurity_profile = \"fapi2\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"self_signed_tls_client_auth\"\nscope = \"{SCOPE}\"\ncertificate_bound = true\n",
        tls_keys(dir.path(), &front)?
    );
    let settings = settings(&text(
        dir.path(),
        ("https://cdr-a.example.org/openehr", unreachable::BASE),
        &tables,
    )?)?;
    let Some(Scheme::Oauth { grant, .. }) = settings.credentials.values().next() else {
        return Err("node A has an oauth grant".into());
    };
    assert!(matches!(grant.profile, SecurityProfile::Fapi2(_)));
    assert!(grant.assertion_keys.is_none());
    assert!(grant.dpop_key().is_none());
    assert_eq!(
        Some(front.client_thumbprint()),
        grant
            .tls
            .presented_certificate()
            .as_ref()
            .map(ClientCertificate::thumbprint)
    );
    assert_eq!(ClientAuth::SelfSignedTlsClientAuth, grant.client_auth);
    Ok(())
}

/// No rendering of the settings shows the client identity or its key.
#[test]
fn no_rendering_shows_the_client_identity() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tables = credentials(
        &tls_keys(dir.path(), &front)?,
        "https://idp.example.org/token",
        "certificate_bound = true\n",
    );
    let settings = settings(&text(
        dir.path(),
        ("https://cdr-a.example.org/openehr", unreachable::BASE),
        &tables,
    )?)?;
    let shown = format!("{settings:?}");
    assert!(!shown.contains("PRIVATE KEY"), "no key is shown");
    let certificate = front
        .client_identity()
        .lines()
        .find(|line| !line.starts_with("-----"))
        .ok_or("the identity holds a certificate")?;
    assert!(
        !shown.contains(certificate),
        "the client certificate is not shown"
    );
    assert!(
        shown.contains(CLIENT_ID),
        "the grant is shown by its client"
    );
    Ok(())
}

/// Node A's section with `tls`, and a FAPI 2.0 grant that authenticates by,
/// and binds its tokens to, the certificate, with `hosts` as its
/// `mtls_alias_hosts`.
fn fapi2_by_certificate(tls: &str, hosts: &str) -> String {
    format!(
        "[credentials.\"node-a-pub\"]\n{tls}\n[credentials.\"node-a-pub\".oauth]\nissuer = \"https://as.cdr-a.example.org\"\nsecurity_profile = \"fapi2\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"tls_client_auth\"\nscope = \"{SCOPE}\"\ncertificate_bound = true\nmtls_alias_hosts = {hosts}\n"
    )
}

/// The configuration of a gateway over node A with `tables`, its files
/// written into `dir`.
fn over_node_a(dir: &Path, tables: &str) -> Result<String, Box<dyn Error>> {
    text(
        dir,
        ("https://cdr-a.example.org/openehr", unreachable::BASE),
        tables,
    )
}

/// The FAPI 2.0 grant takes the hosts its section names for the mutual-TLS
/// aliases (RFC 8705 §5).
#[test]
fn a_fapi2_grant_takes_the_alias_hosts_its_section_names() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tables = fapi2_by_certificate(
        &tls_keys(dir.path(), &front)?,
        r#"["mtls.cdr-a.example.org", "mtls2.cdr-a.example.org:8443"]"#,
    );
    let settings = settings(&over_node_a(dir.path(), &tables)?)?;
    let Some(Scheme::Oauth { grant, .. }) = settings.credentials.values().next() else {
        return Err("node A has an oauth grant".into());
    };
    let SecurityProfile::Fapi2(profile) = &grant.profile else {
        return Err("the grant holds to FAPI 2.0".into());
    };
    assert_eq!(
        vec!["mtls.cdr-a.example.org", "mtls2.cdr-a.example.org:8443"],
        profile
            .alias_hosts()
            .iter()
            .map(AliasHost::as_str)
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// An entry that is no host, a URL or a path among them, is refused naming
/// the key; an alias host is reached over `https` alone, so no scheme is
/// written.
#[test]
fn an_alias_host_that_is_no_host_is_refused_naming_the_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tls = tls_keys(dir.path(), &front)?;
    for entry in [
        "https://mtls.cdr-a.example.org",
        "http://mtls.cdr-a.example.org",
        "mtls.cdr-a.example.org/token",
        "MTLS.cdr-a.example.org",
        "",
    ] {
        let tables = fapi2_by_certificate(&tls, &format!("[\"{entry}\"]"));
        let error = refused(&over_node_a(dir.path(), &tables)?)?;
        assert!(
            smart(&error, |refused| matches!(
                refused,
                SmartError::AliasHost { .. }
            )),
            "{entry}: {error:?}"
        );
    }
    Ok(())
}

/// Alias hosts are refused in a grant that never reads
/// `mtls_endpoint_aliases`: one under plain OAuth 2.0, and a FAPI 2.0 grant
/// that does not use mutual TLS (RFC 8705 §5).
#[test]
fn alias_hosts_without_a_mutual_tls_fapi2_grant_are_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tls = tls_keys(dir.path(), &front)?;
    let hosts = "mtls_alias_hosts = [\"mtls.cdr-a.example.org\"]\n";
    let plain = credentials(&tls, "https://idp.example.org/token", hosts);
    let client_key = dir.path().join("client.pem");
    let dpop = dir.path().join("dpop.pem");
    std::fs::write(&client_key, oauth::Curve::P256.pem()?)?;
    std::fs::write(&dpop, oauth::Curve::P256.pem()?)?;
    let by_key = format!(
        "[credentials.\"node-a-pub\".oauth]\nissuer = \"https://as.cdr-a.example.org\"\nsecurity_profile = \"fapi2\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"private_key_jwt\"\nassertion_audience = \"issuer\"\nprivate_key_file = {}\ndpop_key_file = {}\nscope = \"{SCOPE}\"\n{hosts}",
        toml::Value::String(client_key.display().to_string()),
        toml::Value::String(dpop.display().to_string())
    );
    let error = refused(&over_node_a(dir.path(), &plain)?)?;
    assert!(
        smart(&error, |refused| matches!(
            refused,
            SmartError::Missing { key } if key.contains("mtls_alias_hosts")
        )),
        "{error:?}"
    );
    let error = refused(&over_node_a(dir.path(), &by_key)?)?;
    assert!(
        smart(&error, |refused| matches!(
            refused,
            SmartError::Grant { source } if matches!(
                source.as_ref(),
                ferrohealth_smart::client::TokenError::Fapi2 {
                    source: ferrohealth_smart::client::fapi2::Fapi2Error::AliasHostsUnused
                }
            )
        )),
        "{error:?}"
    );
    Ok(())
}

/// The `oauth` table of a section that names TLS material names no client
/// certificate, key or trust roots of its own: the grant presents the
/// section's identity, read once (RFC 8705 §2, §3).
#[test]
fn a_grant_beside_tls_material_names_no_certificate_of_its_own() -> TestResult {
    let dir = tempfile::tempdir()?;
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let tls = tls_keys(dir.path(), &front)?;
    let identity = toml::Value::String(dir.path().join("node-a-client.pem").display().to_string());
    for extra in [
        format!("client_certificate_file = {identity}\nclient_key_file = {identity}\n"),
        format!("ca_file = {identity}\n"),
    ] {
        let tables = credentials(&tls, "https://idp.example.org/token", &extra);
        let error = refused(&over_node_a(dir.path(), &tables)?)?;
        assert!(
            smart(&error, |refused| matches!(
                refused,
                SmartError::Exclusive { .. }
            )),
            "{extra}: {error:?}"
        );
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("PRIVATE KEY"), "{shown}");
    }
    Ok(())
}

/// A service's credentials section takes no TLS material: the service's own
/// table names it.
#[cfg(feature = "binding-ihe")]
#[test]
fn a_service_credentials_section_takes_no_tls_material() -> TestResult {
    let dir = tempfile::tempdir()?;
    let identity = dir.path().join("client.pem");
    std::fs::write(&identity, "unused")?;
    let text = format!(
        "[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.manager.credentials]\nuser = \"gateway\"\npassword = \"Qz7synthetic\"\ntls = {{ client_identity_file = {} }}\n",
        toml::Value::String(identity.display().to_string())
    );
    let error = refused(&text)?;
    assert!(
        matches!(
            &error,
            ConfigError::TlsFault(TlsFault::OnService { section })
                if section == "pixm.manager[0].credentials"
        ),
        "{error:?}"
    );
    Ok(())
}
