// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The texts client authentication cites, read from the vendored corpora:
//! every section, quotation and claim name the `auth` modules and the
//! security handoff cite is in the pinned text, and SMART on openEHR still
//! declares the DEVELOPMENT status the citations say it has. A re-pin that
//! moves or rewords one fails here, so a citation is never left pointing at
//! text that is not there (#414).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn Error>>;

/// The text of `path`, relative to the repository's `docs/specs/`.
fn vendored(path: &str) -> Result<String, Box<dyn Error>> {
    let file: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "docs",
        "specs",
        path,
    ]
    .iter()
    .collect();
    Ok(std::fs::read_to_string(&file).map_err(|error| format!("{}: {error}", file.display()))?)
}

/// The SMART on openEHR source of ITS-REST Release-1.1.0.
const SMART: &str = "its-rest/docs/smart_app_launch";

#[test]
fn smart_on_openehr_is_development_status_in_the_pinned_release() -> TestResult {
    let manifest = vendored(&format!("{SMART}/manifest_vars.adoc"))?;
    assert!(
        manifest
            .lines()
            .any(|line| line.trim() == ":spec_status: DEVELOPMENT"),
        "the citations call SMART on openEHR DEVELOPMENT status: {manifest}"
    );
    Ok(())
}

#[test]
fn the_cited_smart_sections_and_quotation_are_in_the_pinned_text() -> TestResult {
    let authorization = vendored(&format!("{SMART}/master07-authorization.adoc"))?;
    assert!(
        authorization
            .lines()
            .any(|line| line == "== Context Selection"),
        "master07 §Context Selection"
    );
    let scopes = vendored(&format!("{SMART}/master08-scopes.adoc"))?;
    assert!(
        scopes.lines().any(|line| line == "== Resource Scopes"),
        "master08 §Resource Scopes"
    );
    assert!(
        scopes.contains("would grant access to all registered and ad-hoc AQL queries system-wide"),
        "the quotation behind honouring `system/aql-*` for listed backends only"
    );
    for family in [
        "`template-<templateId>`",
        "`composition-<templateId>`",
        "`aql-<queryName>`",
    ] {
        assert!(scopes.contains(family), "the resource family {family}");
    }
    Ok(())
}

#[test]
fn the_cited_iua_sections_and_claims_are_in_the_pinned_supplement() -> TestResult {
    let iua = vendored("ihe-iua/IHE_ITI_Suppl_IUA.md")?;
    for heading in [
        "**Revision 2.5 - Trial Implementation**",
        "## 3.71 Get Access Token [ITI-71]",
        "###### 3.71.4.2.2.1 JSON Web Token Option",
        "**3.71.4.2.2.1.1 JWT IUA extension**",
        "## 3.72 Incorporate Access Token [ITI-72]",
    ] {
        assert!(
            iua.lines().any(|line| line.trim_end() == heading),
            "the supplement has {heading}"
        );
    }
    for claim in [
        "\"ihe_iua\"",
        "\"subject_organization_id\"",
        "\"purpose_of_use\"",
        "\"person_id\"",
    ] {
        assert!(iua.contains(claim), "the IUA extension defines {claim}");
    }
    Ok(())
}

/// The citations behind the issuer-bound `patient/` opt-in: the `ehrId`
/// claim of master04 §Capabilities, the launch context master07 lists, and
/// the reach of a patient grant in master08 §Resource Scopes.
#[test]
fn the_cited_patient_context_texts_are_in_the_pinned_text() -> TestResult {
    let discovery = vendored(&format!("{SMART}/master04-service_discovery.adoc"))?;
    assert!(
        discovery.lines().any(|line| line == "== Capabilities"),
        "master04 §Capabilities"
    );
    assert!(
        discovery.contains("conveyed via the `ehrId` token claim"),
        "the quotation naming the `ehrId` claim"
    );
    let authorization = vendored(&format!("{SMART}/master07-authorization.adoc"))?;
    assert!(
        authorization.contains("| `ehrId` |"),
        "master07 lists the `ehrId` launch context"
    );
    let scopes = vendored(&format!("{SMART}/master08-scopes.adoc"))?;
    assert!(
        scopes.contains("restricted to data within that patient's EHR"),
        "the quotation behind holding a patient grant to an EHR's data"
    );
    Ok(())
}
