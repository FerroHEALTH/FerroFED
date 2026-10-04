// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[pdqm]` table refused where the demographics step cannot work as
//! configured: no URL, master domain or namespace, a zero timeout, a
//! credential in the URL or a grant in its section, a namespace
//! `[pixm.namespaces]` maps, plain `http` outside development, no audit
//! destination outside development, no cross-reference to resolve the master
//! identity, and a master domain that is no URI; no refusal echoes a
//! credential.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_identity::pdqm::PdqmConfigError;
use ferrofed_server::config::Config;
use ferrofed_server::config::error;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::federation::registry::read_registry;
use ferrofed_server::federation::{Federation, error::FederationError};

use super::{LOCAL, MASTER, pdqm};
use crate::facade::registry;

type TestResult = Result<(), Box<dyn Error>>;

/// A base the tests never reach: the refusals come before any request.
const BASE: &str = "https://pdq.example.org/fhir/";

/// What resolving `text` refuses.
fn refusal(text: &str) -> Result<error::Error, Box<dyn Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new()).and_then(|config| config.resolve()) {
        Ok(_) => Err(format!("the configuration was accepted: {text}").into()),
        Err(error) => Ok(error),
    }
}

/// The `[pdqm]` table of [`pdqm`] with `key = value` set in place of its own.
fn with(key: &str, value: &str) -> String {
    pdqm(BASE, "iti-78")
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{key} =")) {
                format!("{key} = {value}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_table_without_its_url_master_or_namespaces_is_refused() -> TestResult {
    for (text, key) in [
        (with("url", "\"\""), "pdqm.url"),
        (with("master", "\"\""), "pdqm.master"),
        (
            format!("[pdqm]\nurl = \"{BASE}\"\nmaster = \"{MASTER}\"\n"),
            "pdqm.namespaces",
        ),
    ] {
        let error = refusal(&format!("profile = \"development\"\n{text}"))?;
        assert!(
            matches!(&error, error::Error::Missing { key: missing } if missing == key),
            "{key}: {error:?}"
        );
    }
    let error = refusal(&format!(
        "profile = \"development\"\n{}",
        with("timeout_ms", "0")
    ))?;
    assert!(
        matches!(&error, error::Error::Zero { key } if key == "pdqm.timeout_ms"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_credential_in_the_url_or_a_grant_in_its_section_is_refused() -> TestResult {
    let error = refusal(&format!(
        "profile = \"development\"\n{}",
        with("url", "\"https://user:Qz7secret@pdq.example.org/fhir/\"")
    ))?;
    assert!(
        matches!(&error, error::Error::UrlCredentials { key, .. } if key == "pdqm.url"),
        "{error:?}"
    );
    assert!(!format!("{error} {error:?}").contains("Qz7secret"));
    let error = refusal(&format!(
        "profile = \"development\"\n{}\n[pdqm.credentials.oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"gateway\"\nscope = \"system/aql-*.s\"\n",
        pdqm(BASE, "iti-78")
    ))?;
    assert!(
        matches!(&error, error::Error::GrantNotHere { section } if section == "pdqm.credentials"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_namespace_the_cross_reference_maps_is_refused() -> TestResult {
    let error = refusal(&format!(
        "profile = \"development\"\n[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.namespaces]\n\"{LOCAL}\" = \"{LOCAL}\"\n\n{}",
        pdqm(BASE, "iti-78")
    ))?;
    assert!(
        matches!(&error, error::Error::Pdqm { key, .. } if key.contains(LOCAL)),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn plain_http_outside_development_is_refused() -> TestResult {
    let error = refusal(&format!(
        "[audit]\ndestination = \"log\"\n\n{}",
        pdqm("http://pdq.example.org/fhir/", "iti-78")
    ))?;
    let error::Error::Cleartext(cleartext) = &error else {
        return Err(format!("plain http is refused, got {error:?}").into());
    };
    assert_eq!("pdqm.url", cleartext.site.url_key);
    Ok(())
}

#[test]
fn an_audit_destination_is_required_outside_development() -> TestResult {
    let error = refusal(&pdqm(BASE, "iti-78"))?;
    assert!(
        matches!(&error, error::Error::Missing { key } if key == "audit.destination"),
        "PDQm §2:3.78.5.1: {error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_transaction_is_refused() -> TestResult {
    let error = refusal(&format!(
        "profile = \"development\"\n{}",
        with("transaction", "\"iti-21\"")
    ))?;
    assert!(matches!(error, error::Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_budget_that_leaves_no_time_to_resolve_is_refused() -> TestResult {
    let error = refusal(&format!(
        "profile = \"development\"\n[federation]\noverall_timeout_ms = 1000\n\n{}",
        with("timeout_ms", "1000")
    ))?;
    assert!(
        matches!(
            error,
            error::Error::DemographicsBudget {
                timeout_ms: 1000,
                localization_ms: 0,
                overall_ms: 1000
            }
        ),
        "§11.5: the step's budget is a part of the overall one: {error:?}"
    );
    Ok(())
}

/// The settings of the development configuration `tables` over a registry
/// of two members written into `dir`.
fn settings(dir: &Path, tables: &str) -> Result<Settings, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("https://cdr-a.example.org", "https://cdr-b.example.org", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[audit]\ndestination = \"log\"\n\n{tables}"
    );
    Ok(Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?)
}

/// The federation the development configuration `tables` describes over a
/// registry of two members.
fn federation(tables: &str) -> Result<Result<Option<Federation>, FederationError>, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    Ok(Federation::load(&settings(dir.path(), tables)?))
}

/// The `[pixm]` table of a PIX Manager the tests never reach.
fn unreached_pixm() -> String {
    String::from(
        "[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n",
    )
}

#[test]
fn a_reload_carries_the_running_step_over() -> TestResult {
    let dir = tempfile::tempdir()?;
    let boot = settings(
        dir.path(),
        &format!("{}\n{}", unreached_pixm(), pdqm(BASE, "iti-78")),
    )?;
    let running = Federation::load(&boot)?.ok_or("a registry is configured")?;
    assert!(running.demographics().is_some());
    // A reload builds over settings without `[pdqm]`, which takes a restart.
    let fresh = settings(dir.path(), &unreached_pixm())?;
    let next = running
        .reloaded(&fresh, read_registry(&fresh))?
        .ok_or("a registry is configured")?;
    assert!(
        next.demographics().is_some(),
        "the reloaded federation keeps the running step"
    );
    Ok(())
}

#[test]
fn a_step_without_a_cross_reference_is_refused() -> TestResult {
    let built = federation(&pdqm(BASE, "iti-78"))?;
    assert!(
        matches!(built, Err(FederationError::PdqmWithoutResolver)),
        "Annex A §A.2: the master identity needs a resolver: {built:?}"
    );
    Ok(())
}

#[test]
fn a_master_domain_that_is_no_uri_is_refused() -> TestResult {
    let tables = format!("{}\n{}", unreached_pixm(), with("master", "\"not a uri\""));
    let built = federation(&tables)?;
    assert!(
        matches!(built, Err(FederationError::Pdqm(PdqmConfigError::Master))),
        "{built:?}"
    );
    Ok(())
}
