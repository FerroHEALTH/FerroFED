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
    Answer, Assembled, Author, Held, NOT_EXHAUSTIVE, Organisation, Origin, Request, SectionAnswers,
    assemble,
};
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

/// The document the two members' `sections` make.
fn document(sections: &[SectionAnswers]) -> Result<Assembled, Box<dyn Error>> {
    let mappings = Mappings::compile(&[allergies()?])?;
    let request = Request {
        base: BASE.to_owned(),
        identifier: String::from("urn:uuid:5a6b7c8d-0000-4000-8000-000000000001"),
        patient_id: String::from("5a6b7c8d-0000-4000-8000-000000000002"),
        timestamp: String::from("2026-10-06T10:00:00Z"),
        system: String::from("urn:oid:2.999.1"),
        value: String::from("synthetic-subject-689"),
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
