// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The medication controls of the patient summary: a probable duplicate
//! across members marked, a medicine recorded only as an order selected, a
//! vaccination and a medicine placed by an explicit criterion, and what a
//! mapping run could not carry named.

use std::error::Error;

use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::mappings::{Mappings, Source};
use ferrofed_eehrxf::summary::{Answer, Held, SectionAnswers};
use ferrofed_testkit::eps;

use super::{document_mapped, header, section};

/// The mappings of the medication summary, from the synthetic medication use
/// and order, and of the immunisations, from the synthetic vaccination.
fn medications() -> Result<Mappings, Box<dyn Error>> {
    let opt = std::fs::read_to_string(eps::medication_opt())?;
    let source = |section, context: &str| Source {
        section,
        opt: opt.clone(),
        files: eps::medication_mapping_files(),
        context: context.to_owned(),
    };
    Ok(Mappings::compile(&[
        source(Section::MedicationSummary, eps::MEDICATION_CONTEXT),
        source(Section::MedicationSummary, eps::MEDICATION_ORDER_CONTEXT),
        source(Section::Immunisations, eps::IMMUNISATION_CONTEXT),
    ])?)
}

/// A synthetic medicine, by its ATC code, given by the route `route`.
fn medicine<'a>(code: &'a str, route: &'a str) -> eps::Medicine<'a> {
    eps::Medicine {
        code,
        display: "Synthetic medicine",
        route,
    }
}

/// The answers of every section: the medication summary and the
/// immunisations take `a`'s compositions from node A and `b`'s from node B,
/// as a query selecting by a shared archetype would, and every other
/// section nothing.
fn medication_answers(a: &[Held], b: &[Held]) -> Vec<SectionAnswers> {
    Section::ALL
        .into_iter()
        .map(|section| {
            let shared = matches!(section, Section::MedicationSummary | Section::Immunisations);
            let held = |compositions: &[Held]| {
                Answer::Answered(if shared {
                    compositions.to_vec()
                } else {
                    Vec::new()
                })
            };
            SectionAnswers {
                section,
                answers: vec![(0, held(a)), (1, held(b))],
                listed_once: Vec::new(),
            }
        })
        .collect()
}

/// One composition of the synthetic medication template.
fn medication_held(uid: &str, used: &[eps::Medicine<'_>], ordered: &[eps::Medicine<'_>]) -> Held {
    Held {
        uid: uid.to_owned(),
        template: eps::MEDICATION_TEMPLATE.to_owned(),
        composition: eps::medication_composition(uid, used, ordered),
    }
}

/// The resources a section of `bundle` lists, by the `fullUrl`s of its
/// entries.
fn listed<'a>(bundle: &'a serde_json::Value, code: &str) -> Vec<&'a serde_json::Value> {
    let Some(section) = section(bundle, code) else {
        return Vec::new();
    };
    let urls: Vec<&str> = section["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry["reference"].as_str())
                .collect()
        })
        .unwrap_or_default();
    bundle["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| {
                    entry["fullUrl"]
                        .as_str()
                        .is_some_and(|url| urls.contains(&url))
                })
                .map(|entry| &entry["resource"])
                .collect()
        })
        .unwrap_or_default()
}

/// The ATC code of the medicine `resource` names, a `MedicationStatement`'s
/// or an `Immunization`'s.
fn atc(resource: &serde_json::Value) -> Option<&str> {
    resource["medicationCodeableConcept"]["coding"][0]["code"]
        .as_str()
        .or_else(|| resource["vaccineCode"]["coding"][0]["code"].as_str())
}

#[test]
fn one_medicine_two_members_record_alike_is_marked_a_probable_duplicate_and_kept_twice()
-> Result<(), Box<dyn Error>> {
    let a = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a1::cdr-a.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    let b = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013b1::cdr-b.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    let assembled = document_mapped(
        &medication_answers(&[a], &[b]),
        header(),
        &[],
        &medications()?,
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let listed = listed(&bundle, "10160-0");
    assert_eq!(listed.len(), 2, "both entries stay, never merged");
    assert!(listed.iter().all(|entry| atc(entry) == Some("C09AA05")));
    let medications = section(&bundle, "10160-0").ok_or("the medication summary")?;
    let div = medications["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(div.contains("Probable duplicate"), "{div}");
    assert!(div.contains("Synthetic org-a and Synthetic org-b"), "{div}");
    let provenance = bundle["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["resource"]["resourceType"] == "Provenance")
                .count()
        })
        .unwrap_or_default();
    assert!(provenance >= 2, "each entry keeps its Provenance");
    Ok(())
}

#[test]
fn another_route_or_one_member_is_no_probable_duplicate() -> Result<(), Box<dyn Error>> {
    let a = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a2::cdr-a.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    let b = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013b2::cdr-b.example.org::1",
        &[medicine("C09AA05", "20045000")],
        &[],
    );
    let first = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a3::cdr-a.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    let second = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a6::cdr-a.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    for (a, b) in [(vec![a], vec![b]), (vec![first, second], Vec::new())] {
        let assembled =
            document_mapped(&medication_answers(&a, &b), header(), &[], &medications()?)?;
        let bundle = serde_json::to_value(&assembled.bundle)?;
        assert_eq!(listed(&bundle, "10160-0").len(), 2);
        let medications = section(&bundle, "10160-0").ok_or("the medication summary")?;
        let div = medications["text"]["div"].as_str().ok_or("a narrative")?;
        assert!(!div.contains("Probable duplicate"), "{div}");
    }
    Ok(())
}

#[test]
fn a_vaccination_and_a_medicine_each_land_in_their_own_section_by_the_vaccine_code()
-> Result<(), Box<dyn Error>> {
    // Both compositions share the synthetic medication archetype, so both
    // section queries select both; the vaccine code alone places each.
    let vaccination = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a4::cdr-a.example.org::1",
        &[medicine(eps::VACCINE, "20035000")],
        &[],
    );
    let medicine_use = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a7::cdr-a.example.org::1",
        &[medicine("C09AA05", "20053000")],
        &[],
    );
    let assembled = document_mapped(
        &medication_answers(&[vaccination, medicine_use], &[]),
        header(),
        &[],
        &medications()?,
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let medications: Vec<Option<&str>> = listed(&bundle, "10160-0").into_iter().map(atc).collect();
    let immunisations: Vec<Option<&str>> =
        listed(&bundle, "11369-6").into_iter().map(atc).collect();
    assert_eq!(
        medications,
        [Some("C09AA05")],
        "the medicine alone, by its code not being the vaccine's"
    );
    assert_eq!(
        immunisations,
        [Some(eps::VACCINE)],
        "the vaccination alone, by its code"
    );
    Ok(())
}

#[test]
fn a_composition_holding_two_medicines_is_left_out_and_named_never_mixed()
-> Result<(), Box<dyn Error>> {
    let two = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a8::cdr-a.example.org::1",
        &[
            medicine("C09AA05", "20053000"),
            medicine("N02BE01", "20053000"),
        ],
        &[],
    );
    let assembled = document_mapped(
        &medication_answers(&[two], &[]),
        header(),
        &[],
        &medications()?,
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    assert!(
        listed(&bundle, "10160-0").is_empty(),
        "no entry mixes the two medicines"
    );
    let medications = section(&bundle, "10160-0").ok_or("the medication summary")?;
    let div = medications["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(
        div.contains("A record from Synthetic org-a holds 2 entries of openEHR-EHR-EVALUATION."),
        "the record left out is named in the document, never silent: {div}"
    );
    assert!(div.contains("leaves the record out"), "{div}");
    assert_eq!(
        medications["emptyReason"]["coding"][0]["code"], "unavailable",
        "the section does not claim the patient takes no medicine"
    );
    Ok(())
}

#[test]
fn a_composition_holding_a_vaccine_and_a_medicine_is_named_in_both_sections_never_mixed()
-> Result<(), Box<dyn Error>> {
    let both = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a9::cdr-a.example.org::1",
        &[
            medicine(eps::VACCINE, "20035000"),
            medicine("C09AA05", "20053000"),
        ],
        &[],
    );
    let assembled = document_mapped(
        &medication_answers(&[both], &[]),
        header(),
        &[],
        &medications()?,
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    for code in ["10160-0", "11369-6"] {
        assert!(
            listed(&bundle, code).is_empty(),
            "no entry in {code} mixes the two"
        );
        let div = section(&bundle, code)
            .and_then(|found| found["text"]["div"].as_str())
            .ok_or("a narrative")?;
        assert!(
            div.contains(
                "A record from Synthetic org-a holds 2 entries of openEHR-EHR-EVALUATION."
            ),
            "the record left out is named in {code}: {div}"
        );
    }
    Ok(())
}

#[test]
fn a_medicine_recorded_only_as_an_order_reaches_the_medication_summary()
-> Result<(), Box<dyn Error>> {
    let ordered = medication_held(
        "7c4e1d20-0000-4000-8000-0000000013a5::cdr-a.example.org::1",
        &[],
        &[medicine("C09AA05", "20053000")],
    );
    let assembled = document_mapped(
        &medication_answers(&[ordered], &[]),
        header(),
        &[],
        &medications()?,
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let medications = listed(&bundle, "10160-0");
    assert_eq!(
        medications.len(),
        1,
        "the order is never left out of the summary"
    );
    assert_eq!(medications[0]["resourceType"], "MedicationStatement");
    assert_eq!(atc(medications[0]), Some("C09AA05"));
    Ok(())
}
