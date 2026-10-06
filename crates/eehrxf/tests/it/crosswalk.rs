// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary crosswalk holds against the vendored Xt-EHR and
//! HL7 Europe Patient Summary packages, and each way a crosswalk can fail to
//! hold is reported.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::category::Category;
use eehrxf::crosswalk::Crosswalk;
use eehrxf::crosswalk::Defect;
use eehrxf::crosswalk::Gap;
use eehrxf::crosswalk::Populate;
use eehrxf::crosswalk::Row;
use eehrxf::crosswalk::patient_summary::EPS_COMPOSITION;
use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use eehrxf::dataset::DatasetModel;
use eehrxf::dataset::ResourceProfile;

use super::support::eps;
use super::support::xtehr;

/// The two packages the patient summary crosswalk names.
fn packages() -> Result<(DatasetModel, ResourceProfile), Box<dyn Error>> {
    Ok((xtehr()?, eps(EPS_COMPOSITION)?))
}

/// Checks `crosswalk` against the vendored packages.
fn check(crosswalk: &Crosswalk<'_>) -> Result<Vec<Defect>, Box<dyn Error>> {
    let (model, profile) = packages()?;
    Ok(crosswalk.check(model.dataset(crosswalk.root)?, &profile))
}

/// The patient summary rows with `edit` applied to each.
fn rows_with(edit: impl Fn(Row) -> Option<Row>) -> Vec<Row> {
    PATIENT_SUMMARY
        .rows
        .iter()
        .copied()
        .filter_map(edit)
        .collect()
}

#[test]
fn the_patient_summary_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    let defects = check(&PATIENT_SUMMARY)?;
    assert_eq!(defects, []);
    Ok(())
}

#[test]
fn the_patient_summary_category_carries_the_crosswalk() -> Result<(), Box<dyn Error>> {
    let crosswalk = Category::PatientSummary.crosswalk();
    assert!(
        crosswalk.is_some_and(|crosswalk| crosswalk.root == PATIENT_SUMMARY.root),
        "the patient summary names its crosswalk"
    );
    let [root] = Category::PatientSummary.roots() else {
        return Err("the patient summary names one dataset".into());
    };
    assert_eq!(*root, PATIENT_SUMMARY.root);
    Ok(())
}

#[test]
fn every_section_of_the_model_and_every_child_of_it_has_a_row() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    let dataset = model.dataset(PATIENT_SUMMARY.root)?;
    let sections: Vec<&str> = dataset
        .model()
        .elements()
        .iter()
        .filter(|element| {
            element.types() == ["Base"]
                && element
                    .path()
                    .parent()
                    .is_some_and(|parent| parent.as_str() == "EHDSPatientSummary")
                && element.path().as_str() != "EHDSPatientSummary.header"
        })
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(sections.len(), 15, "{sections:?}");
    for element in dataset.model().elements() {
        let path = element.path().as_str();
        let in_section = sections.iter().any(|section| {
            path == *section
                || element
                    .path()
                    .parent()
                    .is_some_and(|parent| parent.as_str() == *section)
        });
        if in_section {
            assert!(PATIENT_SUMMARY.row(path).is_some(), "{path} has a row");
        }
    }
    Ok(())
}

#[test]
fn every_producer_should_element_has_a_row_but_the_presented_form() -> Result<(), Box<dyn Error>> {
    // The presented form, a PDF rendering of the whole summary, is carried by
    // no element of the EPS composition.
    let model = xtehr()?;
    let dataset = model.dataset(PATIENT_SUMMARY.root)?;
    let mut uncovered = Vec::new();
    for element in dataset.model().elements() {
        let path = element.path().as_str();
        if Populate::of(dataset.obligations(), path).is_some()
            && PATIENT_SUMMARY.row(path).is_none()
        {
            uncovered.push(path);
        }
    }
    assert_eq!(uncovered, ["EHDSPatientSummary.presentedForm"]);
    Ok(())
}

#[test]
fn the_eps_sections_left_out_are_the_ones_the_model_lacks() -> Result<(), Box<dyn Error>> {
    // Vital signs and the general patient history are EPS sections with no
    // EHDSPatientSummary counterpart; both are optional.
    let profile = eps(EPS_COMPOSITION)?;
    let named: BTreeSet<&str> = PATIENT_SUMMARY.rows.iter().map(|row| row.eps).collect();
    let left_out: Vec<&str> = profile
        .elements()
        .iter()
        .map(|element| element.path().as_str())
        .filter(|id| {
            id.strip_prefix("Composition.section:")
                .is_some_and(|name| !name.contains('.'))
                && !named.contains(id)
        })
        .collect();
    assert_eq!(
        left_out,
        [
            "Composition.section:sectionVitalSigns",
            "Composition.section:sectionPatientHx",
        ]
    );
    Ok(())
}

#[test]
fn every_required_eps_section_slice_is_covered() -> Result<(), Box<dyn Error>> {
    let profile = eps(EPS_COMPOSITION)?;
    let required: Vec<&str> = profile
        .elements()
        .iter()
        .filter(|element| {
            element
                .path()
                .as_str()
                .strip_prefix("Composition.section:")
                .is_some_and(|name| !name.contains('.'))
                && element.cardinality().is_required()
        })
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(
        required,
        [
            "Composition.section:sectionProblems",
            "Composition.section:sectionAllergies",
            "Composition.section:sectionMedications",
            "Composition.section:sectionProceduresHx",
            "Composition.section:sectionMedicalDevices",
        ]
    );
    for slice in required {
        assert!(
            PATIENT_SUMMARY.rows.iter().any(|row| row.eps == slice),
            "{slice} is covered"
        );
    }
    Ok(())
}

#[test]
fn the_medical_alert_and_the_functional_status_are_open_for_the_clinical_review() {
    assert_eq!(
        PATIENT_SUMMARY.gaps,
        [
            Gap {
                path: "EHDSPatientSummary.alerts.medicalAlert",
                ehn: "A.2.1.2",
            },
            Gap {
                path: "EHDSPatientSummary.functionalStatus",
                ehn: "A.2.3.4",
            },
        ]
    );
}

#[test]
fn a_producer_shall_element_without_a_row_fails() -> Result<(), Box<dyn Error>> {
    let path = "EHDSPatientSummary.allergiesAndIntolerances.allergyIntolerance";
    let rows = rows_with(|row| (row.path != path).then_some(row));
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Uncovered {
            path: path.to_owned()
        }]
    );
    Ok(())
}

#[test]
fn a_row_naming_a_slice_the_profile_lacks_fails() -> Result<(), Box<dyn Error>> {
    let path = "EHDSPatientSummary.problems.problem";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                eps: "Composition.section:sectionProblems.entry:diagnosis",
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Slice {
            path,
            eps: "Composition.section:sectionProblems.entry:diagnosis",
        }]
    );
    Ok(())
}

#[test]
fn an_uncovered_required_section_slice_fails() -> Result<(), Box<dyn Error>> {
    let rows = rows_with(|row| {
        Some(if row.eps == "Composition.section:sectionMedicalDevices" {
            Row {
                eps: "Composition.section:sectionPatientHx",
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::RequiredSlice {
            id: "Composition.section:sectionMedicalDevices".to_owned(),
        }]
    );
    Ok(())
}

#[test]
fn a_producer_obligation_the_package_does_not_state_fails() -> Result<(), Box<dyn Error>> {
    let path = "EHDSPatientSummary.immunisations";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                producer: Some(Populate::Shall),
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Obligation {
            path,
            row: Some(Populate::Shall),
            package: Some(Populate::Should),
        }]
    );
    Ok(())
}

#[test]
fn a_context_profile_the_slice_does_not_target_fails() -> Result<(), Box<dyn Error>> {
    let path = "EHDSPatientSummary.procedures.procedure";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                contexts: &["http://hl7.org/fhir/StructureDefinition/Procedure"],
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Context {
            path,
            eps: "Composition.section:sectionProceduresHx.entry:procedure",
            profile: "http://hl7.org/fhir/StructureDefinition/Procedure",
        }]
    );
    Ok(())
}

#[test]
fn an_element_path_the_model_lacks_fails() -> Result<(), Box<dyn Error>> {
    let mut rows = PATIENT_SUMMARY.rows.to_vec();
    rows.push(Row {
        path: "EHDSPatientSummary.vitalSigns",
        ehn: &[],
        producer: None,
        eps: "Composition.section:sectionVitalSigns",
        contexts: &[],
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Path {
            path: "EHDSPatientSummary.vitalSigns"
        }]
    );
    Ok(())
}

#[test]
fn a_row_given_twice_fails() -> Result<(), Box<dyn Error>> {
    let mut rows = PATIENT_SUMMARY.rows.to_vec();
    let first = *rows.first().ok_or("a first row")?;
    rows.push(first);
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Duplicate {
            path: "EHDSPatientSummary.header"
        }]
    );
    Ok(())
}

#[test]
fn an_ehn_id_the_catalogue_lacks_fails() -> Result<(), Box<dyn Error>> {
    let path = "EHDSPatientSummary.procedures";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                ehn: &["A.2.3.9"],
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Ehn {
            path,
            id: "A.2.3.9"
        }]
    );
    Ok(())
}

#[test]
fn a_gap_that_names_no_row_carrying_its_id_fails() -> Result<(), Box<dyn Error>> {
    let gaps = [Gap {
        path: "EHDSPatientSummary.alerts.medicalAlert",
        ehn: "A.2.3.4",
    }];
    let defects = check(&Crosswalk {
        gaps: &gaps,
        ..PATIENT_SUMMARY
    })?;
    assert_eq!(
        defects,
        [Defect::Gap {
            path: "EHDSPatientSummary.alerts.medicalAlert",
            ehn: "A.2.3.4",
        }]
    );
    Ok(())
}

#[test]
fn a_crosswalk_checked_against_another_profile_fails() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    let profile = eps("http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps")?;
    let defects = PATIENT_SUMMARY.check(model.dataset(PATIENT_SUMMARY.root)?, &profile);
    assert!(
        defects.contains(&Defect::Profile {
            url: "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps".to_owned()
        }),
        "{defects:?}"
    );
    Ok(())
}
