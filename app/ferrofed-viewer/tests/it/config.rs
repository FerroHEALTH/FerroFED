// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration: the defaults, the environment overrides, the `_file`
//! secrets, and every refusal naming its key.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write as _;
use std::time::Duration;

use ferrofed_viewer::config::Config;
use ferrofed_viewer::config::error;
use ferrofed_viewer::config::error::Problem;
use secrecy::ExposeSecret as _;

use crate::support::{WITH_OIDC, settings};

/// The refusal of the configuration `text` writes.
fn refused(text: &str) -> error::Error {
    Config::from_sources(Some(text), &BTreeMap::new())
        .and_then(|config| config.resolve())
        .expect_err("the configuration should have been refused")
}

#[test]
fn the_defaults_resolve_to_a_console_on_loopback_without_sign_in() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert_eq!("127.0.0.1:3000", settings.listen.to_string());
    assert_eq!("http://127.0.0.1:8080/", settings.gateway.base.as_str());
    assert_eq!(Duration::from_secs(30), settings.gateway.timeout);
    assert!(settings.session.secure_cookie);
    assert!(settings.oidc.is_none());
    Ok(())
}

#[test]
fn an_environment_override_replaces_the_file_value() -> Result<(), Box<dyn Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROFED_VIEWER__GATEWAY__BASE_URL"),
            String::from("https://gateway.example.org/fed"),
        ),
        (
            String::from("FERROFED_VIEWER__SESSION__SECURE_COOKIE"),
            String::from("false"),
        ),
    ]);
    let text = "[gateway]\nbase_url = \"https://other.example.org/\"\n";
    let settings = Config::from_sources(Some(text), &environment)?.resolve()?;
    assert_eq!(
        "https://gateway.example.org/fed",
        settings.gateway.base.as_str()
    );
    assert!(!settings.session.secure_cookie);
    Ok(())
}

/// The book page on the console, whose example configuration is held here.
const BOOK_PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/operator-console.md"
);

#[test]
fn the_example_configuration_in_the_book_resolves() -> Result<(), Box<dyn Error>> {
    let page = std::fs::read_to_string(BOOK_PAGE)?;
    let example: String = page
        .split("```toml\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .ok_or("the page carries a TOML example")?
        .to_owned();
    let mut secret = tempfile::NamedTempFile::new()?;
    writeln!(secret, "synthetic-client-secret")?;
    let example = example.replace(
        "/run/secrets/viewer-client-secret",
        &secret.path().display().to_string(),
    );
    let settings = settings(&example)?;
    assert_eq!("0.0.0.0:3000", settings.listen.to_string());
    assert!(settings.oidc.is_some());
    Ok(())
}

#[test]
fn a_parse_refusal_names_the_key_and_the_line_only() {
    let error =
        refused("[server]\nlisten = \"127.0.0.1:3000\"\n\n[gateway]\ntimeout_ms = \"soon\"\n");
    let error::Error::Parse { fault } = &error else {
        panic!("a parse refusal: {error:?}");
    };
    assert_eq!(Some("gateway.timeout_ms"), fault.key.as_deref());
    assert_eq!(Problem::InvalidValue, fault.problem);
    assert_eq!(Some(5), fault.position.map(|(line, _column)| line));
    assert!(!error.to_string().contains("soon"), "{error}");
}

#[test]
fn an_unknown_key_is_refused() {
    let error = refused("[server]\nlisten = \"127.0.0.1:3000\"\nport = 3000\n");
    assert!(matches!(error, error::Error::Parse { .. }), "{error:?}");
}

#[test]
fn the_client_secret_is_read_from_its_file_trimmed() -> Result<(), Box<dyn Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "s3cret-from-a-file")?;
    let text = format!(
        "{WITH_OIDC}client_secret_file = \"{}\"\n",
        file.path().display()
    );
    let settings = settings(&text)?;
    let oidc = settings.oidc.ok_or("the provider is configured")?;
    let secret = oidc.client_secret.ok_or("the secret is read")?;
    assert_eq!("s3cret-from-a-file", secret.expose_secret());
    Ok(())
}

#[test]
fn a_secret_set_inline_and_in_a_file_is_refused() {
    let text =
        format!("{WITH_OIDC}client_secret = \"inline\"\nclient_secret_file = \"/run/secrets/x\"\n");
    let error = refused(&text);
    assert!(
        matches!(&error, error::Error::Conflict { key } if key == "oidc.client_secret"),
        "{error:?}"
    );
}

#[test]
fn an_empty_or_missing_secret_file_is_refused_by_its_key() -> Result<(), Box<dyn Error>> {
    let empty = tempfile::NamedTempFile::new()?;
    let text = format!(
        "{WITH_OIDC}client_secret_file = \"{}\"\n",
        empty.path().display()
    );
    let error = refused(&text);
    assert!(
        matches!(&error, error::Error::EmptySecret { key, .. } if key == "oidc.client_secret_file"),
        "{error:?}"
    );
    let text = format!("{WITH_OIDC}client_secret_file = \"/nonexistent/ferrofed-viewer\"\n");
    let error = refused(&text);
    assert!(
        matches!(&error, error::Error::Secret { key, .. } if key == "oidc.client_secret_file"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_inline_secret_never_shows_in_debug_output() -> Result<(), Box<dyn Error>> {
    let text = format!("{WITH_OIDC}client_secret = \"inline-secret-value\"\n");
    let config = Config::from_sources(Some(&text), &BTreeMap::new())?;
    let shown = format!("{config:?} {:?}", config.resolve()?);
    assert!(!shown.contains("inline-secret-value"), "{shown}");
    Ok(())
}

// RFC 6749 §3.1: the authorization endpoint is reached over TLS.
#[test]
fn a_provider_over_plain_http_is_refused_unless_it_is_on_loopback() -> Result<(), Box<dyn Error>> {
    let remote = WITH_OIDC.replace(
        "https://idp.example.org/realms/ferrofed/auth",
        "http://idp.example.org/realms/ferrofed/auth",
    );
    let error = refused(&remote);
    assert!(
        matches!(&error, error::Error::UrlShape { key, .. } if key == "oidc.authorization_endpoint"),
        "{error:?}"
    );
    let loopback = WITH_OIDC.replace("https://idp.example.org", "http://localhost:8099");
    assert!(settings(&loopback)?.oidc.is_some());
    Ok(())
}

// OpenID Connect Core 1.0 §3.1.2.1: the request carries the `openid` scope.
#[test]
fn a_provider_without_the_openid_scope_is_refused() {
    let error = refused(&WITH_OIDC.replace("\"openid\", ", ""));
    assert!(matches!(error, error::Error::Missing { .. }), "{error:?}");
}

// RFC 6749 §3.1.2: the redirection endpoint carries no fragment.
#[test]
fn a_redirect_uri_with_a_fragment_is_refused() {
    let error = refused(&WITH_OIDC.replace("/auth/callback", "/auth/callback#here"));
    assert!(
        matches!(&error, error::Error::UrlShape { key, .. } if key == "oidc.redirect_uri"),
        "{error:?}"
    );
}

#[test]
fn a_provider_without_a_client_id_is_refused() {
    let error = refused(&WITH_OIDC.replace("client_id = \"ferrofed-viewer\"\n", ""));
    assert!(
        matches!(&error, error::Error::Missing { key } if key == "oidc.client_id"),
        "{error:?}"
    );
}

#[test]
fn a_gateway_url_that_is_not_http_or_carries_userinfo_is_refused() {
    for base in [
        "ftp://gateway.example.org/",
        "https://user:pw@gateway.example.org/",
    ] {
        let error = refused(&format!("[gateway]\nbase_url = \"{base}\"\n"));
        assert!(
            matches!(&error, error::Error::UrlShape { key, .. } if key == "gateway.base_url"),
            "{base}: {error:?}"
        );
    }
}

#[test]
fn a_zero_timeout_or_session_bound_is_refused() {
    for (text, key) in [
        ("[gateway]\ntimeout_ms = 0\n", "gateway.timeout_ms"),
        ("[session]\nidle_timeout_s = 0\n", "session.idle_timeout_s"),
        ("[session]\nmax_sessions = 0\n", "session.max_sessions"),
    ] {
        let error = refused(text);
        assert!(
            matches!(&error, error::Error::Zero { key: named } if named == key),
            "{text}: {error:?}"
        );
    }
}

#[test]
fn a_listen_address_that_does_not_parse_is_refused() {
    let error = refused("[server]\nlisten = \"localhost\"\n");
    assert!(
        matches!(&error, error::Error::Address { key, .. } if key == "server.listen"),
        "{error:?}"
    );
}

/// [`WITH_OIDC`], whose cookies are not `Secure`, with `redirect_uri` as the
/// console's redirection endpoint.
fn insecure_at(redirect_uri: &str) -> String {
    WITH_OIDC.replace(
        "redirect_uri = \"http://127.0.0.1:3000/auth/callback\"",
        &format!("redirect_uri = \"{redirect_uri}\""),
    )
}

// A cookie without `Secure` travels over plain HTTP, so it is admitted only on
// a console that is itself plain HTTP on loopback.
#[test]
fn cookies_without_secure_are_refused_off_a_loopback_console() {
    for redirect_uri in [
        "https://console.example.org/auth/callback",
        "http://console.example.org/auth/callback",
        "https://localhost:3000/auth/callback",
        "http://10.0.0.7:3000/auth/callback",
    ] {
        let error = refused(&insecure_at(redirect_uri));
        assert!(
            matches!(&error, error::Error::InsecureCookie { key } if key == "session.secure_cookie"),
            "{redirect_uri}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("session.secure_cookie = false"),
            "{error}"
        );
    }
}

#[test]
fn cookies_without_secure_are_admitted_on_a_loopback_console() -> Result<(), Box<dyn Error>> {
    for redirect_uri in [
        "http://127.0.0.1:3000/auth/callback",
        "http://127.0.0.2/auth/callback",
        "http://localhost:3000/auth/callback",
        "http://[::1]:3000/auth/callback",
    ] {
        let settings = settings(&insecure_at(redirect_uri))?;
        assert!(!settings.session.secure_cookie, "{redirect_uri}");
    }
    Ok(())
}

#[test]
fn the_cookies_are_secure_by_default_on_an_https_console() -> Result<(), Box<dyn Error>> {
    let text = insecure_at("https://console.example.org/auth/callback")
        .replace("secure_cookie = false\n", "");
    let settings = settings(&text)?;
    assert!(settings.session.secure_cookie);
    Ok(())
}
