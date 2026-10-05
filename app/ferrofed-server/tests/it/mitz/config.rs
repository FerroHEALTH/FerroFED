// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[nl_gf.mitz]` configuration the gateway refuses at load or at boot:
//! a Mitz in clear text or with a credential in its URL, a question the
//! closed authorization question does not take, a holder table that does not
//! match the registry, and a budget past the overall one (§11.5).

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_identity::nl::mitz::MitzConfigError;
use ferrofed_server::binding::Role;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::{Config, error, transport};
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;

use super::{
    ORGANISATION_CLAIM, ORGANISATION_TYPE_CLAIM, PROFESSIONAL_CLAIM, TestResult, URA_B, holders,
    mitz_table,
};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, crossref, registry};

/// The configuration of the gateway under `profile`, with `mitz` as the Mitz
/// table, its registry document written into `dir`.
fn configuration(dir: &Path, profile: &str, mitz: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("https://a.example.org", "https://b.example.org", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(crate::support::signed(&format!(
        "profile = \"{profile}\"\n\n[registry]\ndocument = {document}\n\n[federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n{mitz}"
    )))
}

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, error::Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve()
}

/// The production configuration whose Mitz table is the standard one with
/// `edit` applied.
fn edited(edit: impl Fn(String) -> String) -> Result<(tempfile::TempDir, String), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let table = edit(mitz_table("https://mitz.example.org/vraag", &holders()));
    let text = configuration(dir.path(), "production", &table)?;
    Ok((dir, text))
}

#[test]
fn a_mitz_over_plain_http_is_refused_outside_development() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("https://", "http://"))?;
    match resolved(&text) {
        Err(error::Error::Cleartext(refused)) => {
            assert_eq!("nl_gf.mitz.url", refused.site.url_key);
            Ok(())
        }
        other => Err(format!("the BSN never travels in clear text: {other:?}").into()),
    }
}

#[test]
fn a_mitz_over_plain_http_is_named_under_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = mitz_table("http://mitz.example.org/vraag", &holders());
    let settings = resolved(&configuration(dir.path(), "development", &table)?)?;
    let sites: Vec<String> = transport::check(&settings, None)?
        .into_iter()
        .map(|site| format!("{}: {}", site.url_key, site.payload))
        .collect();
    assert!(
        sites.contains(&"nl_gf.mitz.url: the patient identifiers asked of nl_gf.mitz".to_owned()),
        "{sites:?}"
    );
    Ok(())
}

#[test]
fn a_credential_in_the_mitz_url_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("https://", "https://user:Qz7secret@"))?;
    match resolved(&text) {
        Err(refused @ error::Error::UrlCredentials { .. }) => {
            assert!(!refused.to_string().contains("Qz7secret"), "{refused}");
            Ok(())
        }
        other => Err(format!("a credential goes in its own section: {other:?}").into()),
    }
}

#[test]
fn a_purpose_other_than_treat_or_coc_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("\"TREAT\"", "\"ETREAT\""))?;
    match resolved(&text) {
        Err(error::Error::Mitz { key, .. }) => {
            assert_eq!("nl_gf.mitz.purpose", key);
            Ok(())
        }
        other => Err(format!("§3.2.4.2 takes TREAT or COC: {other:?}").into()),
    }
}

// conformance: CP-26
#[test]
fn the_pseudonym_listed_as_the_bsn_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| {
        table.replace(
            "namespaces = [\"",
            "namespaces = [\"http://fhir.nl/fhir/NamingSystem/pseudo-bsn\", \"",
        )
    })?;
    match resolved(&text) {
        Err(error::Error::Mitz { key, .. }) => {
            assert_eq!("nl_gf.mitz.namespaces", key);
            Ok(())
        }
        other => Err(format!("a pseudonym never reaches Mitz as a BSN: {other:?}").into()),
    }
}

/// The data user is the verified caller, never a configured identity
/// (§3.2.4.2, §13.4), so the table has no place for one.
#[test]
fn a_configured_data_user_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| {
        format!(
            "{table}\n[nl_gf.mitz.data_user]\nura = \"ura-test-0100\"\ntype = \"V6\"\nresponsible = \"professional0001\"\nrole = \"01.015\"\n"
        )
    })?;
    match resolved(&text) {
        Err(error::Error::Parse { .. }) => Ok(()),
        other => Err(format!("no fixed professional stands in for a caller: {other:?}").into()),
    }
}

#[test]
fn a_requester_mapping_without_a_claim_name_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| {
        format!(
            "{table}\n[auth]\naudience = \"urn:example:gateway\"\n\n[[auth.issuer]]\nissuer = \"https://issuer.example.test\"\njwks_uri = \"https://issuer.example.test/jwks\"\n\n[auth.issuer.requester]\nprofessional = \"{PROFESSIONAL_CLAIM}\"\norganisation = \"{ORGANISATION_CLAIM}\"\norganisation_type = \"{ORGANISATION_TYPE_CLAIM}\"\n"
        )
    })?;
    match resolved(&text) {
        Err(error::Error::Missing { key }) => {
            assert_eq!("auth.issuer[0].requester.role", key);
            Ok(())
        }
        other => Err(format!("no claim name has a default: {other:?}").into()),
    }
}

#[test]
fn no_data_category_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("[\"GGC002\"]", "[]"))?;
    match resolved(&text) {
        Err(error::Error::Missing { key }) => {
            assert_eq!("nl_gf.mitz.data_categories", key);
            Ok(())
        }
        other => Err(format!("a question asks about a category: {other:?}").into()),
    }
}

#[test]
fn a_member_without_a_holder_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| {
        let cut = format!("\"node-b\" = {{ type = \"V6\", ura = \"{URA_B}\" }}\n");
        table.replace(&cut, "")
    })?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::NoHolder(member))) => {
            assert_eq!("node-b", member.as_str());
            Ok(())
        }
        other => Err(format!("Mitz could never be asked about node-b: {other:?}").into()),
    }
}

#[test]
fn a_holder_with_no_ura_anywhere_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| table.replace(&format!(", ura = \"{URA_B}\""), ""))?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::NoUra(member))) => {
            assert_eq!("node-b", member.as_str());
            Ok(())
        }
        other => Err(format!("the data holder needs a URA: {other:?}").into()),
    }
}

#[test]
fn a_holder_naming_no_member_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| {
        format!("{table}\"node-z\" = {{ type = \"V6\", ura = \"ura-test-0009\" }}\n")
    })?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::UnknownMember(member))) => {
            assert_eq!("node-z", member.as_str());
            Ok(())
        }
        other => Err(format!("a holder names a registry member: {other:?}").into()),
    }
}

#[test]
fn development_consent_rows_and_mitz_together_refuse_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let rows = format!(
        "\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-b\"\n"
    );
    let tables = format!(
        "{}{rows}{}",
        crossref(&[("node-a", EHR_A)]),
        mitz_table("https://mitz.example.org/vraag", &holders())
    );
    let text = configuration(dir.path(), "development", &tables)?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Conflict(conflict))
            if conflict.role == Role::ConsentPrefilter
                && conflict.sections == ["[nl_gf.mitz]", "[[dev.consent_denied]]"] =>
        {
            Ok(())
        }
        other => Err(format!("N27a: at most one pre-filter is active: {other:?}").into()),
    }
}

#[test]
fn a_prefilter_budget_that_leaves_no_time_to_resolve_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = format!(
        "overall_timeout_ms = 3000\n\n[federation.localization]\ntimeout_ms = 1500\n\n[pdqm]\nurl = \"https://pdq.example.org/fhir/\"\nmaster = \"urn:oid:2.999.1\"\ntimeout_ms = 500\n\n[pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n{}",
        mitz_table("https://mitz.example.org/vraag", &holders())
    );
    match resolved(&configuration(dir.path(), "production", &tables)?) {
        Err(
            refused @ error::Error::PrefilterBudget {
                timeout_ms: 1000,
                demographics_ms: 500,
                localization_ms: 1500,
                overall_ms: 3000,
            },
        ) => {
            let text = refused.to_string();
            for key in [
                "nl_gf.mitz.timeout_ms",
                "pdqm.timeout_ms",
                "federation.localization.timeout_ms",
                "federation.overall_timeout_ms",
            ] {
                assert!(text.contains(key), "{text}");
            }
            Ok(())
        }
        other => {
            Err(format!("§11.5: the three budgets end before the overall one: {other:?}").into())
        }
    }
}

#[test]
fn a_prefilter_budget_alone_past_the_overall_budget_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("timeout_ms = 1000", "timeout_ms = 25000"))?;
    match resolved(&text) {
        Err(error::Error::PrefilterBudget {
            timeout_ms: 25_000,
            demographics_ms: 0,
            localization_ms: 0,
            overall_ms: 25_000,
        }) => Ok(()),
        other => Err(format!(
            "§11.5: the pre-filter's budget is a part of the overall one: {other:?}"
        )
        .into()),
    }
}
