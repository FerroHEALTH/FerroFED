// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth]` and `[trust]` as written: every value the gateway cannot
//! authenticate callers with refuses to boot, naming its key, and a
//! federating gateway that trusts no issuer does not start (§13.1, N25;
//! CP-17 inbound half). `[auth]` is the section `ferrohealth-smart` reads,
//! whose refusals name the crate's rule.
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
use ferrofed_server::config::auth::{AuthFault, AuthMode, Verification};
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

/// Asserts that `ferrofed-smart` refuses `text`'s `[auth]` for the rule
/// whose text is `rule`.
fn refused_by_the_crate(text: &str, rule: &str) -> Result<(), Box<dyn Error>> {
    match refusal(text)? {
        Some(error @ ConfigError::Smart { .. }) => {
            let mut shown = error.to_string();
            let mut cause = Error::source(&error);
            while let Some(source) = cause {
                shown.push_str(": ");
                shown.push_str(&source.to_string());
                cause = source.source();
            }
            assert!(shown.starts_with("auth is refused"), "{shown}");
            assert!(shown.contains(rule), "{rule}: {shown}");
            Ok(())
        }
        other => Err(format!("{text}: expected the crate to refuse {rule}, got {other:?}").into()),
    }
}

/// One trusted issuer at `issuer`, under `[auth]` keys `extra`.
fn issuer(extra: &str, issuer: &str) -> String {
    format!("[auth]\nissuer = \"{issuer}\"\naudience = [\"urn:example:gateway\"]\n{extra}\n")
}

/// The test issuer's identifier.
const ISSUER: &str = "https://issuer.example.test";

#[test]
fn an_issuer_resolves_with_every_default() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(Some(&issuer("", ISSUER)), &BTreeMap::new())?.resolve()?;
    let auth = settings.server.auth;
    assert_eq!(AuthMode::Token, auth.mode);
    assert!(auth.purpose_required, "§13.4: required by default");
    assert_eq!(None, auth.product_scope, "no operator by default");
    let [trusted] = auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    let Verification::KeySet(settings) = &trusted.verification else {
        return Err("verified by its key set".into());
    };
    assert_eq!(ISSUER, settings.issuer);
    assert_eq!(Duration::from_secs(60), settings.clock_skew);
    assert_eq!(
        None, settings.jwks_file,
        "the key set comes from the metadata"
    );
    Ok(())
}

#[test]
fn an_absent_auth_section_trusts_no_issuer() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert_eq!(
        settings.server.auth.issuers,
        Vec::<ferrofed_server::config::auth::IssuerSettings>::new()
    );
    Ok(())
}

#[test]
fn a_disabled_auth_section_is_refused() -> Result<(), Box<dyn Error>> {
    refused_for(
        &issuer("enabled = false", ISSUER),
        "auth.enabled",
        AuthFault::Disabled,
    )
}

// RFC 7519 §4.1.4: "some small leeway, usually no more than a few minutes".
#[test]
fn a_clock_skew_past_five_minutes_is_refused() -> Result<(), Box<dyn Error>> {
    refused_by_the_crate(&issuer("clock_skew_seconds = 301", ISSUER), "clock skew")
}

// RFC 9068 §4: the resource server validates that `aud` names it.
#[test]
fn an_issuer_without_an_audience_is_refused() -> Result<(), Box<dyn Error>> {
    refused_by_the_crate(
        &format!("[auth]\nissuer = \"{ISSUER}\"\n"),
        "name no audience",
    )
}

#[test]
fn an_issuer_verified_two_ways_is_refused() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}\n[[trust.issuer]]\nissuer = \"{ISSUER}\"\nintrospection_endpoint = \"https://issuer.example.test/introspect\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-secret\"\n",
        issuer("", ISSUER)
    );
    refused_for(
        &text,
        "trust.issuer[0].introspection_endpoint",
        AuthFault::TwoVerifications,
    )
}

#[test]
fn a_policy_for_an_issuer_no_one_verifies_is_refused() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}\n[[trust.issuer]]\nissuer = \"https://another.example.test\"\n",
        issuer("", ISSUER)
    );
    refused_for(&text, "trust.issuer[0].issuer", AuthFault::UnknownIssuer)
}

// RFC 8414 §2: the issuer identifier is an `https` URL; loopback `http` is
// the crate's own rule for a development authorization server.
#[test]
fn an_issuer_over_plain_http_to_another_host_is_refused() -> Result<(), Box<dyn Error>> {
    for profile in ["production", "development"] {
        refused_by_the_crate(
            &format!(
                "profile = \"{profile}\"\n{}",
                issuer("", "http://issuer.example.test")
            ),
            "is not an https URL",
        )?;
    }
    assert!(
        refusal(&issuer("", "http://127.0.0.1:8443"))?.is_none(),
        "plain http to loopback resolves"
    );
    Ok(())
}

// SMART App Launch 2.2.0, Scopes and Launch Context, Extensibility: a
// product scope is "a full URI that you control" or "any string starting
// with `__`".
#[test]
fn a_product_scope_in_neither_extension_form_is_refused() -> Result<(), Box<dyn Error>> {
    for scope in ["ferrofed:operator", "operator", "ferrofed operator"] {
        refused_by_the_crate(
            &issuer(&format!("product_scope = \"{scope}\""), ISSUER),
            "is neither a full URI nor a string starting with __",
        )?;
    }
    for scope in [
        "__ferrofed.operator",
        "https://ferrofed.example.org/operator",
    ] {
        let settings = Config::from_sources(
            Some(&issuer(&format!("product_scope = \"{scope}\""), ISSUER)),
            &BTreeMap::new(),
        )?
        .resolve()?;
        assert_eq!(Some(scope), settings.server.auth.product_scope.as_deref());
    }
    Ok(())
}

#[test]
fn a_key_the_gateway_reads_nowhere_is_refused() -> Result<(), Box<dyn Error>> {
    for (key, value) in [
        ("scopeless_subjects", "[\"synthetic-service\"]"),
        ("capabilities", "[\"permission-v1\"]"),
        ("scopes_supported", "[\"user/aql-*.s\"]"),
    ] {
        refused_for(
            &issuer(&format!("{key} = {value}"), ISSUER),
            &format!("auth.{key}"),
            AuthFault::Unread,
        )?;
    }
    Ok(())
}

/// The keys `[auth]` held before it was `ferrohealth-smart`'s are unknown
/// keys now, refused by name with no compatibility reading.
#[test]
fn a_key_of_the_former_auth_section_is_refused() -> Result<(), Box<dyn Error>> {
    for written in [
        "clock_skew_s = 60",
        "key_set_max_age_s = 600",
        "key_set_refetch_s = 30",
        "fetch_timeout_ms = 5000",
        "mode = \"token\"",
    ] {
        let text = issuer(written, ISSUER);
        let error = Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .err()
            .ok_or("refused")?;
        assert!(
            matches!(error, ConfigError::Parse { .. }),
            "{written}: {error:?}"
        );
    }
    let inline = format!(
        "[auth]\naudience = [\"urn:example:gateway\"]\n\n[[auth.issuer]]\nissuer = \"{ISSUER}\"\njwks = \"[1, 2]\"\n"
    );
    let error = Config::from_sources(Some(&inline), &BTreeMap::new())
        .and_then(|config| config.resolve())
        .err()
        .ok_or("refused")?;
    assert!(
        matches!(error, ConfigError::Parse { .. }),
        "an inline key set is no key: {error:?}"
    );
    Ok(())
}

#[test]
fn a_second_introspected_issuer_is_refused() -> Result<(), Box<dyn Error>> {
    let one = |name: &str| {
        format!(
            "\n[[trust.issuer]]\nissuer = \"https://{name}.example.test\"\nintrospection_endpoint = \"https://{name}.example.test/introspect\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-secret\"\n"
        )
    };
    let text = format!(
        "[auth]\naudience = [\"urn:example:gateway\"]\n{}{}",
        one("first"),
        one("second")
    );
    refused_for(
        &text,
        "trust.issuer[1].introspection_endpoint",
        AuthFault::SeveralIntrospection,
    )
}

#[test]
fn the_edge_mode_needs_one_key_set_issuer_and_its_section_needs_the_mode()
-> Result<(), Box<dyn Error>> {
    refused_for(
        "[auth]\naudience = [\"urn:example:gateway\"]\n\n[trust]\nmode = \"edge\"\n\n[trust.edge]\nheader = \"ferrofed-edge-assertion\"\n",
        "auth.issuer",
        AuthFault::EdgeIssuer,
    )?;
    refused_for(
        &format!(
            "{}\n[trust.edge]\nheader = \"ferrofed-edge-assertion\"\n",
            issuer("", "https://edge.example.test")
        ),
        "trust.edge",
        AuthFault::EdgeWithoutMode,
    )?;
    let edge = format!(
        "{}\n[trust]\nmode = \"edge\"\n\n[trust.edge]\nheader = \"ferrofed-edge-assertion\"\n",
        issuer("", "https://edge.example.test")
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
    let untrusting = format!("{toml}\n[auth]\naudience = [\"urn:example:gateway\"]\n");
    let output = binary(&["config", "check"], &untrusting)?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[auth] names no issuer"), "{stderr}");
    let trusting = format!("{toml}\n{}", issuer("", ISSUER));
    let output = binary(&["config", "check"], &trusting)?;
    assert_eq!(
        Some(0),
        output.status.code(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
