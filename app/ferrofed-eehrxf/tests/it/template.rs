// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Each section's archetypes held to the vendored openEHR International
//! Patient Summary template: the query names every entry archetype the
//! template sections it draws on name, and none they do not.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

use eehrxf::crosswalk::patient_summary::SOURCES;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::section_query::Archetype;

type TestResult = Result<(), Box<dyn Error>>;

/// The template source, vendored by `scripts/vendor/openehr-ckm.sh`.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/openehr-ckm-ips/international-patient-summary.oet"
);

/// The archetype that heads each of the template's sections.
const SECTION: &str = "openEHR-EHR-SECTION.adhoc.v1";

/// The RM classes of the entries a section holds.
const ENTRIES: [&str; 5] = [
    "OBSERVATION",
    "EVALUATION",
    "INSTRUCTION",
    "ACTION",
    "ADMIN_ENTRY",
];

/// The value of the attribute `name` in `tag`, its XML entities read.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let rest = tag.get(start..)?;
    let value = rest.get(..rest.find('"')?)?;
    Some(value.replace("&amp;", "&"))
}

/// Every entry archetype of each template section, by section name.
fn template_sections() -> Result<BTreeMap<String, BTreeSet<String>>, Box<dyn Error>> {
    let text = std::fs::read_to_string(Path::new(TEMPLATE))?;
    let mut sections = BTreeMap::new();
    for block in text.split("<Content ").skip(1) {
        let block = block.split("</Content>").next().ok_or("an empty section")?;
        let head = format!(
            " {}",
            block.split('>').next().ok_or("a section with no tag")?
        );
        assert_eq!(
            Some(SECTION),
            attribute(&head, "archetype_id").as_deref(),
            "each template section is an ad hoc heading"
        );
        let name = attribute(&head, "name").ok_or("a section with no name")?;
        let mut entries = BTreeSet::new();
        for item in block.split("archetype_id=\"").skip(1) {
            let id = item.split('"').next().ok_or("an unterminated id")?;
            let class = id
                .strip_prefix("openEHR-EHR-")
                .and_then(|rest| rest.split('.').next())
                .ok_or("an archetype id outside openEHR-EHR")?;
            if ENTRIES.contains(&class) {
                entries.insert(id.to_owned());
            }
        }
        sections.insert(name, entries);
    }
    Ok(sections)
}

#[test]
fn the_vendored_template_reads_as_fourteen_sections() -> TestResult {
    let sections = template_sections()?;
    assert_eq!(14, sections.len(), "{:?}", sections.keys());
    assert!(sections.contains_key("Allergies & Intolerances"));
    Ok(())
}

#[test]
fn each_section_selects_exactly_the_entry_archetypes_its_template_sections_name_but_those_placed()
-> TestResult {
    let template = template_sections()?;
    // NOTE: eehrxf crosswalk::patient_summary::SOURCES: an archetype a source names is placed by
    // the section of FerroBRIDGE's template that holds it, never by this template's.
    let placed: BTreeSet<String> = SOURCES
        .iter()
        .map(|source| source.archetype.to_owned())
        .collect();
    for section in Section::ALL {
        let mut named = BTreeSet::new();
        for name in section.template_sections() {
            let entries = template
                .get(*name)
                .ok_or_else(|| format!("{section:?}: the template has no section {name}"))?;
            named.extend(entries.iter().filter(|id| !placed.contains(*id)).cloned());
        }
        let selected: BTreeSet<String> = section
            .archetypes()
            .iter()
            .map(|archetype| archetype.id.to_owned())
            .collect();
        assert_eq!(named, selected, "{section:?}");
        assert_eq!(
            section.archetypes().len(),
            selected.len(),
            "{section:?}: an archetype named twice"
        );
    }
    Ok(())
}

#[test]
fn each_archetype_is_contained_as_the_class_its_id_names() {
    for section in Section::ALL {
        assert!(!section.archetypes().is_empty(), "{section:?}");
        for Archetype { class, id } in section.archetypes() {
            assert!(
                id.starts_with(&format!("openEHR-EHR-{class}.")),
                "{section:?}: {id} is no {class}"
            );
        }
    }
}

#[test]
fn every_template_section_with_entries_feeds_a_query_or_is_a_gap() -> TestResult {
    let drawn: BTreeSet<&str> = Section::ALL
        .iter()
        .flat_map(|section| section.template_sections().iter().copied())
        .collect();
    let undrawn: Vec<String> = template_sections()?
        .into_keys()
        .filter(|name| !drawn.contains(name.as_str()))
        .collect();
    assert_eq!(
        vec!["Functional Status".to_owned()],
        undrawn,
        "only the functional status gap (eHN A.2.3.4) is left for the clinical safety review"
    );
    Ok(())
}

#[test]
fn each_placed_archetype_is_selected_under_the_section_the_component_places_it_in() {
    let placed = |section: Section| {
        section
            .placed()
            .into_iter()
            .map(|placed| (placed.section, placed.archetype.id))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        placed(Section::MedicationSummary),
        [
            (
                "openEHR-EHR-SECTION.adhoc.v1",
                "openEHR-EHR-INSTRUCTION.medication_order.v3"
            ),
            (
                "openEHR-EHR-SECTION.adhoc.v1",
                "openEHR-EHR-ACTION.medication.v1"
            )
        ]
    );
    assert_eq!(
        placed(Section::Immunisations),
        [(
            "openEHR-EHR-SECTION.immunisation_list.v0",
            "openEHR-EHR-ACTION.medication.v1"
        )]
    );
    for section in Section::ALL {
        if !matches!(section, Section::MedicationSummary | Section::Immunisations) {
            assert!(section.placed().is_empty(), "{section:?}");
        }
        for placed in section.placed() {
            assert!(
                placed
                    .archetype
                    .id
                    .starts_with(&format!("openEHR-EHR-{}.", placed.archetype.class)),
                "{section:?}: {placed:?}"
            );
            let held = SOURCES.iter().any(|source| {
                source.section == placed.section && source.archetype == placed.archetype.id
            });
            assert!(held, "{section:?}: {placed:?} is a source of the component");
        }
    }
}

/// FerroHEALTH/FerroBRIDGE#1696: a medication action is selected by the
/// section that holds it, the medication summary's ad hoc section for the
/// medication summary and the vaccination list for the immunisations.
#[test]
fn a_medication_action_is_selected_by_the_section_that_holds_it() {
    let medication = Section::MedicationSummary.aql();
    assert!(
        medication.contains("SECTION s2[openEHR-EHR-SECTION.adhoc.v1] CONTAINS INSTRUCTION a2[openEHR-EHR-INSTRUCTION.medication_order.v3]"),
        "{medication}"
    );
    assert!(
        medication.contains("SECTION s3[openEHR-EHR-SECTION.adhoc.v1] CONTAINS ACTION a3[openEHR-EHR-ACTION.medication.v1]"),
        "{medication}"
    );
    assert!(
        !medication.contains("openEHR-EHR-SECTION.immunisation_list.v0"),
        "the medication summary never selects a vaccine by its section: {medication}"
    );
    let immunisations = Section::Immunisations.aql();
    assert!(
        immunisations.contains("SECTION s1[openEHR-EHR-SECTION.immunisation_list.v0] CONTAINS ACTION a1[openEHR-EHR-ACTION.medication.v1]"),
        "{immunisations}"
    );
    for aql in [&medication, &immunisations] {
        assert!(
            openehr_query::parser::parse_str(aql).is_ok(),
            "the query parses: {aql}"
        );
    }
}
