// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No secret reaches an error or a log: a sentinel client secret, set inline,
//! in a `_file` and from the environment, is absent from every load
//! refusal's message, `Debug` rendering and cause chain, and from everything
//! the console traces at `TRACE` while it loads, starts and serves.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write as _;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use ferrofed_viewer::command::chain;
use ferrofed_viewer::config::Config;
use ferrofed_viewer::config::error;
use ferrofed_viewer::server::{ViewerState, router};
use http::Request;
use tower::ServiceExt as _;

/// The value no output may carry.
const SENTINEL: &str = "SENTINEL-c0ffee-0d15ea5e";

/// A configuration with a provider on loopback and no secret yet.
const PROVIDER: &str = r#"
[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/auth"
token_endpoint = "http://127.0.0.1:9/token"
jwks_uri = "http://127.0.0.1:9/jwks.json"
client_id = "ferrofed-viewer"
redirect_uri = "http://127.0.0.1:3000/auth/callback"
"#;

/// The refusal of `text` with `environment`, resolved as the binary does.
fn refusal(text: &str, environment: &[(&str, &str)]) -> error::Error {
    let environment: BTreeMap<String, String> = environment
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    Config::from_sources(Some(text), &environment)
        .and_then(|config| config.resolve())
        .expect_err("the configuration should have been refused")
}

/// Asserts that neither the message, nor `Debug`, nor the cause chain of
/// `error` carries the sentinel.
fn holds_no_secret(case: &str, error: &error::Error) {
    for shown in [error.to_string(), format!("{error:?}"), chain(error)] {
        assert!(!shown.contains(SENTINEL), "{case}: {shown}");
    }
}

#[test]
fn no_load_refusal_shows_a_secret() -> Result<(), Box<dyn Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "{SENTINEL}")?;
    let secret_file = file.path().display().to_string();
    let cases: Vec<Case<'_>> = vec![
        (
            "a syntax error on the secret's line",
            format!("{PROVIDER}client_secret = \"{SENTINEL}\n"),
            vec![],
        ),
        (
            "the secret as a value of the wrong type",
            format!("[gateway]\ntimeout_ms = \"{SENTINEL}\"\n"),
            vec![],
        ),
        (
            "the secret set twice",
            format!("{PROVIDER}client_secret = \"{SENTINEL}\"\nclient_secret = \"{SENTINEL}\"\n"),
            vec![],
        ),
        (
            "the secret under a misspelt key",
            format!("{PROVIDER}client_secrte = \"{SENTINEL}\"\n"),
            vec![],
        ),
        (
            "the secret as a mistyped environment override",
            String::new(),
            vec![("FERROFED_VIEWER__GATEWAY__TIMEOUT_MS", SENTINEL)],
        ),
        (
            "the secret from the environment beside a secret file",
            format!("{PROVIDER}client_secret_file = \"{secret_file}\"\n"),
            vec![("FERROFED_VIEWER__OIDC__CLIENT_SECRET", SENTINEL)],
        ),
        (
            "a secret file with another key refused",
            format!(
                "{}client_secret_file = \"{secret_file}\"\n",
                PROVIDER.replace("/auth/callback", "/auth/callback#here")
            ),
            vec![],
        ),
        (
            "an inline secret with another key refused",
            format!(
                "{}client_secret = \"{SENTINEL}\"\n",
                PROVIDER.replace("https://idp", "http://idp")
            ),
            vec![],
        ),
    ];
    for (case, text, environment) in &cases {
        holds_no_secret(case, &refusal(text, environment));
    }
    Ok(())
}

/// One refusal case: what it shows, the file text, and the environment.
type Case<'a> = (&'a str, String, Vec<(&'a str, &'a str)>);

/// A writer every clone of which appends to one buffer.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_poisoned| std::io::Error::other("the capture buffer is poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn nothing_traced_while_the_console_loads_starts_and_serves_shows_a_secret()
-> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "{SENTINEL}")?;
    let from_file = format!(
        "[session]\nsecure_cookie = false\n{PROVIDER}client_secret_file = \"{}\"\n",
        file.path().display()
    );
    tracing::subscriber::with_default(subscriber, || -> Result<(), Box<dyn Error>> {
        tracing::trace!("capture probe");
        let inline = format!("{PROVIDER}client_secret = \"{SENTINEL}\"\n");
        let environment = BTreeMap::from([(
            String::from("FERROFED_VIEWER__OIDC__CLIENT_SECRET"),
            SENTINEL.to_owned(),
        )]);
        Config::from_sources(Some(&inline), &BTreeMap::new())?.resolve()?;
        Config::from_sources(Some(PROVIDER), &environment)?.resolve()?;
        let settings = Config::from_sources(Some(&from_file), &BTreeMap::new())?.resolve()?;
        refusal(&format!("{PROVIDER}client_secret = \"{SENTINEL}\n"), &[]);
        let service = router(ViewerState::new(settings)?);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            for path in ["/", "/health", "/login", "/auth/callback?code=c&state=s"] {
                service
                    .clone()
                    .oneshot(Request::get(path).body(Body::empty())?)
                    .await?;
            }
            Ok::<(), Box<dyn Error>>(())
        })
    })?;
    let output = String::from_utf8(
        captured
            .0
            .lock()
            .map_err(|_poisoned| "the capture buffer is poisoned")?
            .clone(),
    )?;
    assert!(output.contains("capture probe"), "nothing was captured");
    assert!(!output.contains(SENTINEL), "{output}");
    Ok(())
}
