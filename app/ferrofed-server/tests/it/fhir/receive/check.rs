// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What `config check` says about the Art 14(1) categories the FHIR face
//! receives (Regulation (EU) 2025/327 Art 14(1), Annex II 2.2): each
//! received category with its member, and the categories no member
//! receives, the set the capability statement names.

use super::{receiving, text};
use crate::facade::{NAMESPACE, registry};
use crate::fhir::{TestResult, UNREACHED_SUPPLIER, fhir_tables};
use crate::run::binary;
use crate::support::unasked_pixm;

/// The standard output of `config check` over the development gateway
/// with the face `fhir`, after asserting that it passed.
fn checked(fhir: &str) -> Result<String, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let resolver = unasked_pixm(&registry(
        "http://a.example.org",
        "http://b.example.org",
        "",
    ));
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org", ""),
        &resolver,
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
    let prefix = "ferrofed: note: [[fhir.receive]] names no receiving member for ";
    let note = stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .ok_or_else(|| format!("no unreceived note: {stdout}"))?;
    let (codes, consequence) = note
        .split_once(": ")
        .ok_or_else(|| format!("a note with no consequence: {note}"))?;
    let codes: Vec<&str> = codes.split(", ").collect();
    assert!(codes.contains(&"Patient-Summaries"), "{note}");
    let expected = if codes.len() == 1 {
        "the FHIR face refuses a document of this Art 14(1) category"
    } else {
        "the FHIR face refuses a document of any of these Art 14(1) categories"
    };
    assert_eq!(expected, consequence, "{note}");
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
