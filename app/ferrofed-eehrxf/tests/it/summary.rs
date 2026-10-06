// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document assembled from what two synthetic members
//! answered to the section queries: held to the vendored HL7 Europe Patient
//! Summary profiles, every member named, nothing a mapping does not cover
//! dropped in silence, and every mapping held to the crosswalk.

use std::error::Error;
use std::path::PathBuf;

use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::mappings::{Mappings, MappingsError, Source};
use ferrofed_eehrxf::summary::{
    Answer, Assembled, AssemblyError, Author, DATA_ABSENT_REASON, Held, NOT_EXHAUSTIVE,
    Organisation, Origin, Request, SectionAnswers, assemble,
};
use ferrofed_identity::role::header::{PatientHeader, PersonName, PostalAddress, Telecom};
use ferrofed_testkit::eps;

/// The FHIR base the test documents name their entries under.
const BASE: &str = "https://gateway.example.org/fhir";

/// The mapping of the allergies section from the synthetic template.
fn allergies() -> Result<Source, Box<dyn Error>> {
    Ok(Source {
        section: Section::AllergiesAndIntolerances,
        opt: std::fs::read_to_string(eps::opt())?,
        files: eps::mapping_files().to_vec(),
        context: eps::CONTEXT.to_owned(),
    })
}

/// A synthetic organisation `id`.
fn organisation(id: &str) -> Organisation {
    Organisation {
        id: id.to_owned(),
        name: Some(format!("Synthetic {id}")),
        identifiers: vec![(String::from("urn:oid:2.999.9"), id.to_owned())],
    }
}

/// Two members, node A and node B.
fn origins() -> Vec<Origin> {
    vec![
        Origin {
            endpoint: String::from("node-a-pub"),
            organisation: organisation("org-a"),
        },
        Origin {
            endpoint: String::from("node-b-pub"),
            organisation: organisation("org-b"),
        },
    ]
}

/// One composition of `template` with the uid `uid`.
fn held(uid: &str, template: &str) -> Held {
    Held {
        uid: uid.to_owned(),
        template: template.to_owned(),
        composition: eps::allergy_composition(uid, template, "Synthetic substance"),
    }
}

/// The answers of every section: node A answers the allergies section with
/// `allergies` and every other section with nothing, and node B answers as
/// `b` says.
fn answers(allergies: &[Held], b: &Answer) -> Vec<SectionAnswers> {
    Section::ALL
        .into_iter()
        .map(|section| {
            let a = if section == Section::AllergiesAndIntolerances {
                Answer::Answered(allergies.to_vec())
            } else {
                Answer::Answered(Vec::new())
            };
            SectionAnswers {
                section,
                answers: vec![(0, a), (1, b.clone())],
            }
        })
        .collect()
}

/// A synthetic header, as an identity binding would hold it.
fn header() -> PatientHeader {
    PatientHeader {
        names: vec![PersonName {
            purpose: Some(String::from("official")),
            family: Some(String::from("SYNTHETIC-FAMILY")),
            given: vec![String::from("SYNTHETIC-GIVEN")],
            ..PersonName::default()
        }],
        birth_date: Some(String::from("1970-01-01")),
        gender: Some(String::from("unknown")),
        addresses: vec![PostalAddress {
            lines: vec![String::from("1 Synthetic Street")],
            city: Some(String::from("Synthetic City")),
            country: Some(String::from("NL")),
            ..PostalAddress::default()
        }],
        telecoms: vec![Telecom {
            system: Some(String::from("email")),
            value: Some(String::from("patient@example.org")),
            purpose: Some(String::from("home")),
        }],
    }
}

/// The document the two members' `sections` make.
fn document(sections: &[SectionAnswers]) -> Result<Assembled, Box<dyn Error>> {
    document_headed(sections, header())
}

/// The document the two members' `sections` make, about the patient
/// `header` describes.
fn document_headed(
    sections: &[SectionAnswers],
    header: PatientHeader,
) -> Result<Assembled, Box<dyn Error>> {
    let mappings = Mappings::compile(&[allergies()?])?;
    let request = Request {
        base: BASE.to_owned(),
        identifier: String::from("urn:uuid:5a6b7c8d-0000-4000-8000-000000000001"),
        patient_id: String::from("5a6b7c8d-0000-4000-8000-000000000002"),
        timestamp: String::from("2026-10-06T10:00:00Z"),
        system: String::from("urn:oid:2.999.1"),
        value: String::from("synthetic-subject-689"),
        header,
    };
    let author = Author {
        product: String::from("FerroFED"),
        version: String::from("0.0.0"),
        operator: organisation("operator"),
    };
    Ok(assemble(
        &request,
        &author,
        (&origins(), sections),
        &mappings,
    )?)
}

/// The sections of the document's composition, by LOINC code.
fn section<'a>(bundle: &'a serde_json::Value, code: &str) -> Option<&'a serde_json::Value> {
    bundle["entry"][0]["resource"]["section"]
        .as_array()?
        .iter()
        .find(|section| section["code"]["coding"][0]["code"] == code)
}

#[test]
fn a_document_from_two_members_meets_the_eps_profiles() -> Result<(), Box<dyn Error>> {
    let assembled = document(&answers(
        &[held(
            "7c4e1d20-0000-4000-8000-0000000000a1::cdr-a.example.org::1",
            eps::TEMPLATE,
        )],
        &Answer::Answered(Vec::new()),
    ))?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let allergies = section(&bundle, "48765-2").ok_or("the allergies section")?;
    assert_eq!(allergies["entry"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        allergies["author"][0]["reference"],
        format!("{BASE}/Organization/member-0"),
        "the section names the member whose data it holds"
    );
    let problems = section(&bundle, "11450-4").ok_or("the problems section")?;
    assert_eq!(
        problems["emptyReason"]["coding"][0]["code"], "nilknown",
        "every member answered, none with a problem"
    );
    let div = problems["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(div.contains(NOT_EXHAUSTIVE), "{div}");
    let composition = &bundle["entry"][0]["resource"];
    assert_eq!(
        composition["author"],
        serde_json::json!([
            { "reference": format!("{BASE}/Device/gateway") },
            { "reference": format!("{BASE}/Organization/operator") }
        ]),
        "eHN PS A.1.4.3, A.1.5: the gateway and its operator write it"
    );
    assert!(composition["attester"].is_null(), "no one attests it");
    let provenance = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "Provenance")
        .ok_or("one Provenance per mapped run")?;
    assert_eq!(
        provenance["resource"]["agent"][0]["onBehalfOf"]["reference"],
        format!("{BASE}/Organization/member-0")
    );
    assert!(
        !text.contains("cdr-b.example.org"),
        "nothing of node B, which held nothing"
    );
    Ok(())
}

#[test]
fn the_profile_check_finds_what_a_broken_document_lacks() -> Result<(), Box<dyn Error>> {
    let assembled = document(&answers(&[], &Answer::Answered(Vec::new())))?;
    let mut bundle = serde_json::to_value(&assembled.bundle)?;
    let composition = bundle
        .pointer_mut("/entry/0/resource")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("a composition")?;
    composition.remove("title");
    composition.remove("subject");
    if let Some(sections) = composition
        .get_mut("section")
        .and_then(serde_json::Value::as_array_mut)
    {
        sections.retain(|section| section["code"]["coding"][0]["code"] != "48765-2");
    }
    bundle["type"] = serde_json::Value::from("collection");
    let findings = eps::check(&bundle.to_string())?;
    for expected in [
        "Bundle.type is no document",
        "Composition.title below 1",
        "Composition.subject below 1",
        "Composition.section:sectionAllergies below 1",
    ] {
        assert!(
            findings.iter().any(|finding| finding == expected),
            "{expected}: {findings:#?}"
        );
    }
    Ok(())
}

#[test]
fn a_silent_member_is_named_and_its_sections_are_unavailable() -> Result<(), Box<dyn Error>> {
    let assembled = document(&answers(
        &[],
        &Answer::Silent {
            status: String::from("time-out"),
        },
    ))?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    for code in ["48765-2", "11450-4", "10160-0", "47519-4", "46264-8"] {
        let empty = section(&bundle, code).ok_or("a required section")?;
        assert_eq!(
            empty["emptyReason"]["coding"][0]["code"], "unavailable",
            "{code}: never nilknown while a member is silent"
        );
        let div = empty["text"]["div"].as_str().ok_or("a narrative")?;
        assert!(div.contains("Synthetic org-b (time-out)"), "{div}");
    }
    Ok(())
}

#[test]
fn a_composition_no_mapping_covers_is_reported_never_dropped() -> Result<(), Box<dyn Error>> {
    let uncovered = "7c4e1d20-0000-4000-8000-0000000000a2::cdr-a.example.org::1";
    let assembled = document(&answers(
        &[held(uncovered, "ferrofed.eehrxf.unmapped.v1")],
        &Answer::Answered(Vec::new()),
    ))?;
    assert!(
        assembled.unmapped.contains(&(0, uncovered.to_owned())),
        "{:?}",
        assembled.unmapped
    );
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let allergies = section(&bundle, "48765-2").ok_or("the allergies section")?;
    assert_eq!(allergies["emptyReason"]["coding"][0]["code"], "unavailable");
    let div = allergies["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(
        div.contains(
            "1 record(s) of the template ferrofed.eehrxf.unmapped.v1 from Synthetic org-a"
        ),
        "{div}"
    );
    Ok(())
}

#[test]
fn a_mapping_to_a_profile_the_section_does_not_take_is_refused() -> Result<(), Box<dyn Error>> {
    let mut problems = allergies()?;
    problems.section = Section::Problems;
    let refused = Mappings::compile(&[problems]);
    assert!(
        matches!(
            refused,
            Err(MappingsError::Profile {
                section: "problems",
                ..
            })
        ),
        "{refused:?}"
    );
    let example = Source {
        files: vec![
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/eehrxf/tests/fixtures/mapping/ferrofed_allergy.yml"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/eehrxf/tests/fixtures/mapping/ferrofed_allergy.context.yml"),
        ],
        context: String::from("ferrofed_allergy.context"),
        ..allergies()?
    };
    let refused = Mappings::compile(&[example]);
    assert!(
        matches!(refused, Err(MappingsError::Profile { .. })),
        "an example.org profile is no EPS entry profile: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_context_the_files_do_not_declare_is_refused() -> Result<(), Box<dyn Error>> {
    let absent = Source {
        context: String::from("ferrofed_absent.context"),
        ..allergies()?
    };
    assert!(matches!(
        Mappings::compile(&[absent]),
        Err(MappingsError::Compile { .. })
    ));
    Ok(())
}

/// The `Patient` entry of `bundle`.
fn patient(bundle: &serde_json::Value) -> Option<&serde_json::Value> {
    bundle["entry"]
        .as_array()?
        .iter()
        .map(|entry| &entry["resource"])
        .find(|resource| resource["resourceType"] == "Patient")
}

#[test]
fn the_header_is_the_one_the_identity_binding_holds() -> Result<(), Box<dyn Error>> {
    let assembled = document(&answers(&[], &Answer::Answered(Vec::new())))?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let patient = patient(&bundle).ok_or("a Patient")?;
    assert_eq!(
        patient["name"],
        serde_json::json!([{
            "use": "official",
            "family": "SYNTHETIC-FAMILY",
            "given": ["SYNTHETIC-GIVEN"]
        }]),
        "eHN PS A.1.1.2, A.1.1.3"
    );
    assert_eq!(patient["birthDate"], "1970-01-01", "eHN PS A.1.1.4");
    assert_eq!(patient["gender"], "unknown", "eHN PS A.1.1.5");
    assert_eq!(
        patient["address"],
        serde_json::json!([{
            "line": ["1 Synthetic Street"],
            "city": "Synthetic City",
            "country": "NL"
        }]),
        "eHN PS A.1.2.1"
    );
    assert_eq!(
        patient["telecom"],
        serde_json::json!([{
            "system": "email",
            "value": "patient@example.org",
            "use": "home"
        }]),
        "eHN PS A.1.2.1.8"
    );
    assert_eq!(
        patient["identifier"][0]["value"], "synthetic-subject-689",
        "the identifier the summary was asked by"
    );
    Ok(())
}

#[test]
fn an_element_the_binding_does_not_hold_is_absent_never_invented() -> Result<(), Box<dyn Error>> {
    let named = PatientHeader {
        names: vec![PersonName {
            text: Some(String::from("SYNTHETIC NAME")),
            ..PersonName::default()
        }],
        ..PatientHeader::default()
    };
    let assembled = document_headed(&answers(&[], &Answer::Answered(Vec::new())), named)?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let patient = patient(&bundle).ok_or("a Patient")?;
    assert!(
        patient["birthDate"].is_null(),
        "no birth date is written: {patient}"
    );
    assert_eq!(
        patient["_birthDate"]["extension"],
        serde_json::json!([{ "url": DATA_ABSENT_REASON, "valueCode": "unknown" }]),
        "eHN PS Art 10(5): the required birth date is stated unknown"
    );
    for absent in ["gender", "address", "telecom"] {
        assert!(patient[absent].is_null(), "{absent} is left out: {patient}");
    }
    Ok(())
}

#[test]
fn a_header_with_no_name_writes_no_document() -> Result<(), Box<dyn Error>> {
    let unnamed = PatientHeader {
        names: vec![PersonName {
            purpose: Some(String::from("official")),
            ..PersonName::default()
        }],
        birth_date: Some(String::from("1970-01-01")),
        ..PatientHeader::default()
    };
    let refused = document_headed(&answers(&[], &Answer::Answered(Vec::new())), unnamed);
    let error = refused.err().ok_or("a document with no name was written")?;
    assert!(
        matches!(
            error.downcast_ref::<AssemblyError>(),
            Some(AssemblyError::Unnamed)
        ),
        "EPS ips-pat-1: {error}"
    );
    Ok(())
}
