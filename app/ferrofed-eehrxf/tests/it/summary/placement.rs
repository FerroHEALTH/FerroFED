// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The placement of a medication entry by the template section that holds
//! it (`eehrxf` `crosswalk::patient_summary::SOURCES`): one composition of
//! FerroBRIDGE's patient summary template holding a medicine ordered in its
//! medication summary section and a vaccine administered in its vaccination
//! list section, mapped by the EPS project `eehrxf` ships, lists each in its
//! own section only.

use std::error::Error;

use eehrxf::mappings::EPS;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::mappings::{Mappings, Source};
use ferrofed_eehrxf::summary::{Answer, Held, SectionAnswers};
use ferrofed_testkit::medication;

use super::{document_mapped, header, section};

/// The immunisations context of the EPS project, which it compiles on its
/// own.
const IMMUNIZATION: &str = "ferrobridge_eu_eps_immunization.context";

/// The synthetic medicine's code.
const MEDICINE: &str = "SYN-MEDICINE-0001";

/// The synthetic vaccine's code.
const VACCINE: &str = "SYN-VACCINE-0001";

/// The EPS project's two contexts, each feeding its section, over the
/// vendored patient summary template, the project's files under `dir`.
fn mappings(dir: &std::path::Path) -> Result<Mappings, Box<dyn Error>> {
    let written = medication::patient_summary_template()?;
    let files = EPS.write_to(dir)?;
    let medication = EPS
        .contexts()
        .first()
        .copied()
        .ok_or("the EPS project names a context")?;
    let source = |section, context: &str| Source {
        section,
        opt: written.template.clone(),
        archetypes: written.archetypes.clone(),
        files: files.clone(),
        context: context.to_owned(),
    };
    Ok(Mappings::compile(&[
        source(Section::MedicationSummary, medication),
        source(Section::Immunisations, IMMUNIZATION),
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
