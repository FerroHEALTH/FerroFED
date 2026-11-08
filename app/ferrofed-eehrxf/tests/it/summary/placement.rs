// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The placement of a medication entry by the template section that holds
//! it (`eehrxf` `crosswalk::patient_summary::SOURCES`): one composition of
//! FerroBRIDGE's patient summary template holding a medicine, ordered or
//! taken, in its medication summary section and a vaccine administered in
//! its vaccination list section, mapped by the root context of the EPS
//! project `eehrxf` ships, lists each in its own section only.

use std::error::Error;

use eehrxf::mappings::EPS;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::mappings::{Mappings, Source};
use ferrofed_eehrxf::summary::{Answer, Held, SectionAnswers};
use ferrofed_testkit::medication;

use super::{document_mapped, header, section};

/// The synthetic medicine's code.
const MEDICINE: &str = "SYN-MEDICINE-0001";

/// The synthetic vaccine's code.
const VACCINE: &str = "SYN-VACCINE-0001";

/// The EPS project's root context feeding the medication summary and the
/// immunisations, over the vendored patient summary template, the project's
/// files under `dir`.
fn mappings(dir: &std::path::Path) -> Result<Mappings, Box<dyn Error>> {
    let written = medication::patient_summary_template()?;
    let files = EPS.write_to(dir)?;
    let [root] = EPS.contexts() else {
        return Err("the EPS project names one root context".into());
    };
    let source = |section, context: &str| Source {
        section,
        opt: written.template.clone(),
        archetypes: written.archetypes.clone(),
        files: files.clone(),
        context: context.to_owned(),
    };
    Ok(Mappings::compile(&[
        source(Section::MedicationSummary, root),
        source(Section::Immunisations, root),
    ])?)
}

/// The answers of every section, the medication summary and the
/// immunisations each holding `held` from node A, as both their queries
/// select a composition holding an entry in each section.
fn answers(held: &Held) -> Vec<SectionAnswers> {
    Section::ALL
        .into_iter()
        .map(|section| {
            let selected = matches!(section, Section::MedicationSummary | Section::Immunisations);
            SectionAnswers {
                section,
                answers: vec![(
                    0,
                    Answer::Answered(if selected {
                        vec![held.clone()]
                    } else {
                        Vec::new()
                    }),
                )],
                listed_once: Vec::new(),
            }
        })
        .collect()
}

/// The resource types the section coded `code` of `bundle` lists.
fn listed<'a>(bundle: &'a serde_json::Value, code: &str) -> Vec<&'a str> {
    let Some(found) = section(bundle, code) else {
        return Vec::new();
    };
    let urls: Vec<&str> = found["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["reference"].as_str())
        .collect();
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["fullUrl"]
                .as_str()
                .is_some_and(|url| urls.contains(&url))
        })
        .filter_map(|entry| entry["resource"]["resourceType"].as_str())
        .collect()
}

#[test]
fn one_composition_holding_a_medicine_and_a_vaccine_lists_each_in_its_own_section_only()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let uid = "5a1c3e20-0000-4000-8000-000000001551::cdr-a.example.org::1";
    let held = Held {
        uid: uid.to_owned(),
        template: medication::PATIENT_SUMMARY_TEMPLATE.to_owned(),
        composition: medication::summary_composition(uid, (MEDICINE, VACCINE)),
    };
    let assembled = document_mapped(&answers(&held), header(), &[], &mappings(dir.path())?)?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    assert_eq!(
        vec!["MedicationStatement"],
        listed(&bundle, "10160-0"),
        "the medicine alone, in the medication summary"
    );
    assert_eq!(
        vec!["Immunization"],
        listed(&bundle, "11369-6"),
        "the vaccine alone, in the immunisations"
    );
    Ok(())
}

/// FerroHEALTH/FerroBRIDGE#1696: a medicine the medication summary section
/// records as a medication action is a medication summary entry, and the
/// vaccine the vaccination list records as one stays an immunisation.
#[test]
fn a_medication_action_under_the_medication_summary_is_a_medication_summary_entry()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let uid = "5a1c3e20-0000-4000-8000-000000001742::cdr-a.example.org::1";
    let held = Held {
        uid: uid.to_owned(),
        template: medication::PATIENT_SUMMARY_TEMPLATE.to_owned(),
        composition: medication::summary_composition_taken(uid, (MEDICINE, VACCINE)),
    };
    let assembled = document_mapped(&answers(&held), header(), &[], &mappings(dir.path())?)?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    assert_eq!(
        vec!["MedicationStatement"],
        listed(&bundle, "10160-0"),
        "the medicine taken, in the medication summary"
    );
    assert_eq!(
        vec!["Immunization"],
        listed(&bundle, "11369-6"),
        "the vaccine alone, in the immunisations"
    );
    let text = serde_json::to_string(&bundle)?;
    assert!(text.contains(MEDICINE), "the statement names the medicine");
    assert_eq!(
        1,
        assembled.transformations.len(),
        "one run of the root context, counted once: {:?}",
        assembled.transformations
    );
    let findings = ferrofed_testkit::eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    Ok(())
}

/// FerroHEALTH/FerroBRIDGE#1712: an element of the composition the root
/// context's run leaves unread is declared a `Loss::Unread` warning, which
/// the run's report counts once and each section that takes from the record
/// names in its narrative, so it is never dropped in silence (hazard I-03).
#[test]
fn an_element_the_run_leaves_unread_is_counted_and_named() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let uid = "5a1c3e20-0000-4000-8000-000000001743::cdr-a.example.org::1";
    let held = Held {
        uid: uid.to_owned(),
        template: medication::PATIENT_SUMMARY_TEMPLATE.to_owned(),
        composition: medication::summary_composition(uid, (MEDICINE, VACCINE)),
    };
    let assembled = document_mapped(&answers(&held), header(), &[], &mappings(dir.path())?)?;
    let [report] = assembled.transformations.as_slice() else {
        return Err(format!("one report: {:?}", assembled.transformations).into());
    };
    assert!(report.dropped().count() > 0, "{report:?}");
    let bundle = serde_json::to_value(&assembled.bundle)?;
    for code in ["10160-0", "11369-6"] {
        let div = section(&bundle, code).map(|found| found["text"]["div"].to_string());
        assert!(
            div.as_deref().is_some_and(|div| div.contains(
                "element(s) of a record this section takes from could not be carried into the summary"
            )),
            "{code}: {div:?}"
        );
    }
    Ok(())
}
