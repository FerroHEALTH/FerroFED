// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth]` as written: every value the gateway cannot authenticate callers
//! with refuses to boot, naming its key, and a federating gateway that
//! trusts no issuer does not start (§13.1, N25; CP-17 inbound half).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write as _;
use std::time::Duration;

use ferrofed_server::EXIT_CONFIG;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::{AuthFault, AuthMode, KeySource, Verification};
use ferrofed_server::config::error::Error as ConfigError;

use crate::run::binary;

/// The refusal of `text`, or `None` when it resolves.
fn refusal(text: &str) -> Result<Option<ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err())
}

/// Asserts that `text` is refused for `fault` at `key`.
fn refused_for(text: &str, key: &str, fault: AuthFault) -> Result<(), Box<dyn Error>> {
    match refusal(text)? {
        Some(ConfigError::Auth {
            key: named,
            fault: found,
        }) => {
            assert_eq!((key, fault), (named.as_str(), found), "{text}");
            Ok(())
        }
        other => Err(format!("{text}: expected {fault:?} at {key}, got {other:?}").into()),
    }
}

/// One trusted issuer by its key set URL, under `[auth]` keys `extra`.
fn issuer(extra: &str, uri: &str) -> String {
    format!(
        "[auth]\naudience = \"urn:example:gateway\"\n{extra}\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\njwks_uri = \"{uri}\"\n"
    )
}

#[test]
fn an_issuer_resolves_with_every_default() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(
        Some(&issuer("", "https://issuer.example.test/jwks")),
        &BTreeMap::new(),
    )?
    .resolve()?;
    let auth = settings.server.auth;
    assert_eq!(AuthMode::Token, auth.mode);
    assert_eq!(Duration::from_secs(60), auth.clock_skew);
    assert!(auth.purpose_required, "§13.4: required by default");
    let [trusted] = auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    assert!(matches!(
        &trusted.verification,
        Verification::KeySet(KeySource::Uri(url)) if url.as_str() == "https://issuer.example.test/jwks"
    ));
    Ok(())
}

#[test]
fn an_absent_auth_section_trusts_no_issuer() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert!(settings.server.auth.issuers.is_empty());
    Ok(())
}

#[test]
fn a_clock_skew_past_five_minutes_is_refused() -> Result<(), Box<dyn Error>> {
    refused_for(
        &issuer("clock_skew_s = 301", "https://issuer.example.test/jwks"),
        "auth.clock_skew_s",
        AuthFault::SkewTooLarge,
    )
}

#[test]
fn an_issuer_without_an_audience_is_refused() -> Result<(), Box<dyn Error>> {
    let text = "[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\njwks_uri = \"https://issuer.example.test/jwks\"\n";
    match refusal(text)? {
        Some(ConfigError::Missing { key }) => {
            assert_eq!("auth.audience", key);
            Ok(())
        }
        other => Err(format!("expected the missing audience, got {other:?}").into()),
    }
}

#[test]
fn an_issuer_naming_two_ways_to_verify_is_refused() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}jwks_file = \"/etc/ferrofed/jwks.json\"\n",
        issuer("", "https://issuer.example.test/jwks")
    );
    refused_for(&text, "auth.issuer[0]", AuthFault::Verification)
}

#[test]
fn a_key_set_over_plain_http_to_another_host_is_refused() -> Result<(), Box<dyn Error>> {
    for profile in ["production", "development"] {
        let text = format!(
            "profile = \"{profile}\"\n{}",
            issuer("", "http://issuer.example.test/jwks")
        );
        match refusal(&text)? {
            Some(ConfigError::TrustAnchor(refused)) => {
                assert_eq!("auth.issuer[0].jwks_uri", refused.key, "{profile}");
            }
            other => {
                return Err(
                    format!("{profile}: a key set in the clear is refused: {other:?}").into(),
                );
            }
        }
    }
    assert!(
        refusal(&issuer("", "http://127.0.0.1:8443/jwks"))?.is_none(),
        "plain http to loopback resolves"
    );
    Ok(())
}

// RFC 6749 §3.3: the operator scope is one scope token, so a space, a quote,
// a backslash or nothing is refused.
#[test]
fn an_operator_scope_that_is_not_one_scope_token_is_refused() -> Result<(), Box<dyn Error>> {
    for scope in [
        "",
        "ferrofed operator",
        "ferrofed\\\\operator",
        "ferrofed\\\"operator",
    ] {
        let text = format!(
            "{}operator_scope = \"{scope}\"\n",
            issuer("", "https://issuer.example.test/jwks")
        );
        refused_for(
            &text,
            "auth.issuer[0].operator_scope",
            AuthFault::OperatorScope,
        )?;
    }
    let text = format!(
        "{}operator_scope = \"ferrofed:operator\"\n",
        issuer("", "https://issuer.example.test/jwks")
    );
    assert!(refusal(&text)?.is_none(), "one scope token resolves");
    Ok(())
}

#[test]
fn an_inline_key_set_that_is_no_jwk_set_is_refused() -> Result<(), Box<dyn Error>> {
    let text = "[auth]\naudience = \"urn:example:gateway\"\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\njwks = \"[1, 2]\"\n";
    refused_for(text, "auth.issuer[0].jwks", AuthFault::KeySet)
}

#[test]
fn a_second_introspected_issuer_is_refused() -> Result<(), Box<dyn Error>> {
    let one = |name: &str| {
        format!(
            "\n[[auth.issuer]]\nissuer = \"https://{name}.example.test\"\nintrospection_endpoint = \"https://{name}.example.test/introspect\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-secret\"\n"
        )
    };
    let text = format!(
        "[auth]\naudience = \"urn:example:gateway\"\n{}{}",
        one("first"),
        one("second")
    );
    refused_for(
        &text,
        "auth.issuer[1].introspection_endpoint",
        AuthFault::SeveralIntrospection,
    )
}

#[test]
fn the_edge_mode_needs_one_key_set_issuer_and_its_section_needs_the_mode()
-> Result<(), Box<dyn Error>> {
    refused_for(
        "[auth]\nmode = \"edge\"\naudience = \"urn:example:gateway\"\n\n[auth.edge]\nheader = \"ferrofed-edge-assertion\"\n",
        "auth.issuer",
        AuthFault::EdgeIssuer,
    )?;
    refused_for(
        &format!(
            "{}\n[auth.edge]\nheader = \"ferrofed-edge-assertion\"\n",
            issuer("", "https://edge.example.test/jwks")
        ),
        "auth.edge",
        AuthFault::EdgeWithoutMode,
    )?;
    let edge = format!(
        "{}\n[auth.edge]\nheader = \"ferrofed-edge-assertion\"\n",
        issuer("mode = \"edge\"", "https://edge.example.test/jwks")
    );
    let settings = Config::from_sources(Some(&edge), &BTreeMap::new())?.resolve()?;
    assert!(
        matches!(settings.server.auth.mode, AuthMode::Edge(ref name) if name == "ferrofed-edge-assertion")
    );
    Ok(())
}

/// §13.1, N25: a gateway that federates and trusts no issuer would refuse
/// every caller, so `config check` refuses it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_federating_gateway_without_an_issuer_does_not_start() -> Result<(), Box<dyn Error>> {
    let mut document = tempfile::NamedTempFile::new()?;
    document.write_all(
        b"[[organisation]]\nid = \"org-a\"\n\n\
          [[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n\
          [[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"http://127.0.0.1:9/openehr\"\n\
          connection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
    )?;
    let path = toml::Value::String(document.path().display().to_string());
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n\n[registry]\ndocument = {path}\n\n\
         [federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n"
    );
    let untrusting = format!("{toml}\n[auth]\naudience = \"urn:example:gateway\"\n");
    let output = binary(&["config", "check"], &untrusting)?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[[auth.issuer]]"), "{stderr}");
    let trusting = format!("{toml}\n{}", issuer("", "https://issuer.example.test/jwks"));
    let output = binary(&["config", "check"], &trusting)?;
    assert_eq!(
        Some(0),
        output.status.code(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
