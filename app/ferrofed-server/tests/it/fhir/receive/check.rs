// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What `config check` says about the Art 14(1) categories the FHIR face
//! receives (Regulation (EU) 2025/327 Art 14(1), Annex II 2.2): each
//! received category with its member, and the categories no member
//! receives, the set the capability statement names.

use super::{receiving, rows, text};
use crate::facade::{EHR_A, NAMESPACE, PATIENT};
use crate::fhir::{TestResult, UNREACHED_SUPPLIER, fhir_tables};
use crate::run::binary;

/// The standard output of `config check` over the development gateway
/// with the face `fhir`, after asserting that it passed.
fn checked(fhir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org", ""),
        &crossref,
        fhir,
    )?;
    let output = binary(&["config", "check"], &text)?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(stdout)
}

#[test]
fn a_received_category_is_named_with_its_member() -> TestResult {
    let stdout = checked(&receiving("node-a", &[NAMESPACE]))?;
    assert!(
        stdout.contains(
            "ferrofed: note: [[fhir.receive]] Patient-Summaries: the FHIR face receives documents of this Art 14(1) category and writes them to the member node-a\n"
        ),
        "{stdout}"
    );
    assert!(
        !stdout.contains("names no receiving member for Patient-Summaries"),
        "{stdout}"
    );
    Ok(())
}

#[test]
fn a_category_with_no_member_is_named() -> TestResult {
    let stdout = checked(&fhir_tables(UNREACHED_SUPPLIER))?;
    assert!(
        stdout.contains(
            "ferrofed: note: [[fhir.receive]] names no receiving member for Patient-Summaries: the FHIR face refuses a document of this Art 14(1) category\n"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("writes them to the member"), "{stdout}");
    Ok(())
}

#[test]
fn no_face_names_no_category() -> TestResult {
    let stdout = checked("")?;
    assert!(!stdout.contains("[[fhir.receive]]"), "{stdout}");
    assert!(
        stdout.contains("ferrofed: the configuration is valid"),
        "{stdout}"
    );
    Ok(())
}
