// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `server.public_url`: the gateway's public base URL, named once. The
//! audience, the JWK Set's URL and the PMIR callback default from it, and a
//! JWK Set URL or callback that names another route than the gateway serves
//! under it is refused by its key. No specification governs the key: our own
//! design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;

use crate::support::{JWKS_URI, signing_key_file};

type TestResult = Result<(), Box<dyn Error>>;

fn resolve(text: &str) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve())
}

/// A gateway under `base` at `public`, signing with the suite's key, with
/// the `[signing]` keys `signing`.
fn gateway(base: &str, public: &str, signing: &str) -> String {
    format!(
        "[server]\nbase_path = \"{base}\"\npublic_url = \"{public}\"\n\n[signing]\nkey_file = {}\n{signing}",
        toml::Value::String(signing_key_file().to_owned())
    )
}

#[test]
fn the_audience_and_the_jwk_set_url_default_from_the_public_url() -> TestResult {
    let settings = resolve(&gateway("/fed", "https://gateway.example.org/fed", ""))?
        .map_err(|error| error.to_string())?;
    assert_eq!(
        Some("https://gateway.example.org/fed"),
        settings.server.auth.audience.as_deref()
    );
    let signing = settings.signing.ok_or("[signing] is resolved")?;
    assert_eq!(
        "https://gateway.example.org/fed/.well-known/jwks.json",
        signing.jwks_uri.as_str()
    );
    Ok(())
}

#[test]
fn a_written_audience_is_taken_as_written() -> TestResult {
    let text = format!(
        "{}\n[auth]\naudience = \"ferrofed\"\n",
        gateway("/", "https://gateway.example.org/", "")
    );
    let settings = resolve(&text)?.map_err(|error| error.to_string())?;
    assert_eq!(Some("ferrofed"), settings.server.auth.audience.as_deref());
    Ok(())
}

#[test]
fn a_jwk_set_url_that_names_the_served_route_is_accepted() -> TestResult {
    let settings = resolve(&gateway(
        "/fed",
        "https://gateway.example.org/fed/",
        "jwks_uri = \"https://gateway.example.org:443/fed/.well-known/jwks.json\"\n",
    ))?
    .map_err(|error| error.to_string())?;
    assert!(settings.signing.is_some());
    Ok(())
}

#[test]
fn a_jwk_set_url_that_misses_the_base_path_is_refused_by_its_key() -> TestResult {
    let refused = resolve(&gateway(
        "/fed",
        "https://gateway.example.org/fed",
        "jwks_uri = \"https://gateway.example.org/.well-known/jwks.json\"\n",
    ))?
    .err()
    .ok_or("a JWK Set URL outside the base is refused")?;
    match &refused {
        ConfigError::PublicUrlDisagrees { key, served } => {
            assert_eq!("signing.jwks_uri", key);
            assert_eq!(
                "https://gateway.example.org/fed/.well-known/jwks.json",
                served
            );
        }
        other => panic!("refused as a disagreement: {other}"),
    }
    Ok(())
}

#[test]
fn a_public_url_whose_path_is_not_the_base_path_is_refused() -> TestResult {
    let refused = resolve(&gateway("/fed", "https://gateway.example.org/", ""))?
        .err()
        .ok_or("a public URL outside the base is refused")?;
    assert!(
        matches!(refused, ConfigError::PublicUrlPath { .. }),
        "{refused}"
    );
    assert!(
        refused.to_string().contains("server.public_url"),
        "{refused}"
    );
    let refused = resolve(&gateway("/", "https://gateway.example.org/?a=1", ""))?
        .err()
        .ok_or("a public URL with a query is refused")?;
    assert!(matches!(refused, ConfigError::PublicUrlForm), "{refused}");
    Ok(())
}

#[test]
fn without_a_public_url_each_copy_is_read_as_written() -> TestResult {
    let text = format!(
        "[server]\nbase_path = \"/fed\"\n\n[signing]\nkey_file = {}\njwks_uri = \"{JWKS_URI}\"\n",
        toml::Value::String(signing_key_file().to_owned())
    );
    let settings = resolve(&text)?.map_err(|error| error.to_string())?;
    assert_eq!(
        JWKS_URI,
        settings.signing.ok_or("[signing]")?.jwks_uri.as_str()
    );
    assert_eq!(None, settings.server.auth.audience);
    Ok(())
}

#[cfg(feature = "binding-ihe")]
mod pmir {
    use super::{TestResult, resolve};
    use ferrofed_server::config::error::Error as ConfigError;

    /// A development gateway at `public` with `[pmir]` sending to `callback`.
    fn gateway(
        dir: &std::path::Path,
        callback: &str,
        public: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let text = crate::pmir::text(dir, "http://127.0.0.1:9/fhir/", callback, "")?;
        Ok(format!(
            "{text}\n[server]\nbase_path = \"/fed\"\npublic_url = \"{public}\"\n\n[signing]\nkey_file = {}\n",
            toml::Value::String(crate::support::signing_key_file().to_owned())
        ))
    }

    #[test]
    fn the_callback_defaults_to_the_feed_route_under_the_public_url() -> TestResult {
        let dir = tempfile::tempdir()?;
        let text = gateway(dir.path(), "", "http://gateway.example.org/fed")?;
        let settings = resolve(&text)?.map_err(|error| error.to_string())?;
        let pmir = settings.pmir.ok_or("[pmir] is resolved")?;
        assert_eq!(
            format!("http://gateway.example.org/fed{}", pmir.path),
            pmir.callback_url.as_str()
        );
        Ok(())
    }

    #[test]
    fn a_callback_that_misses_the_base_path_is_refused_by_its_key() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = crate::pmir::PATH;
        let text = gateway(
            dir.path(),
            &format!("http://gateway.example.org{path}"),
            "http://gateway.example.org/fed",
        )?;
        let refused = resolve(&text)?
            .err()
            .ok_or("a callback outside the base is refused")?;
        assert!(
            matches!(&refused, ConfigError::PublicUrlDisagrees { key, .. } if key == "pmir.callback_url"),
            "{refused}"
        );
        Ok(())
    }
}
