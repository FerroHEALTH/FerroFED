// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary the component writes, read back through its own
//! receive path: decoded, held to the R4 document rules and to the EPS
//! `Bundle` and `Composition` profiles, its `Provenance` entries included.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::document::EPS_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use fhir_types::codec::{Json, Value};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_rm::v1_2::composition::composition::Composition;
use openehr_rm::v1_2::data_types::encapsulated::dv_encapsulated::DvEncapsulated;

use super::claimed::eu_mapping;
use super::document::document_mapped;
use super::support::{emit, eps};

/// The canonical URL of the EPS `Composition` profile.
const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

// FHIR R4 profiling, slicing rules: `bundle-eu-eps` slices `Bundle.entry` open, and FHIR R4
// References, logical references: a `Provenance.entity.what` by identifier alone is valid.
#[test]
fn the_summary_the_component_writes_is_received_by_it() -> Result<(), Box<dyn Error>> {
    let bundle = document_mapped(&eu_mapping()?, 2)?;
    let provenance = bundle
        .entry
        .iter()
        .filter(|entry| matches!(entry.resource, Some(Resource::Provenance(_))))
        .count();
    assert_eq!(2, provenance, "one Provenance per mapped run");
    let text = serde_json::to_string(&bundle)?;
    let document = ReceivedDocument::read(&text)?;
    document.check(&eps(EPS_BUNDLE)?, &eps(EPS_COMPOSITION)?)?;
    emit("eehrxf-summary-two-runs", &text)?;
    Ok(())
}

// FHIR R4 Bundle references and Resource.meta.profile: the summary names one subject, and its
// mapped entry claims the profile a receiver selects its mapping by.
#[test]
fn the_summary_the_component_writes_maps_back_to_openehr() -> Result<(), Box<dyn Error>> {
    let mapping = eu_mapping()?;
    let bundle = document_mapped(&mapping, 1)?;
    let text = serde_json::to_string(&bundle)?;
    let document = ReceivedDocument::read(&text)?;
    document.check(&eps(EPS_BUNDLE)?, &eps(EPS_COMPOSITION)?)?;
    let now = "2026-10-06T10:00:00Z";
    let settings = Settings::new("Device/ferrofed-eehrxf-test", now)
        .with_defaults(Defaults::at(now).with_language("en").with_territory("NL"));
    let received = mapping.to_openehr(&document, &settings)?;
    assert_eq!(received.template_id(), "ferrofed.eehrxf.allergy.v1");
    assert!(
        received.composition().contains("Synthetic substance one"),
        "the summary's allergy is mapped back into the composition"
    );
    let composition: Composition = serde_json::from_str(received.composition())?;
    let original = composition
        .feeder_audit
        .and_then(|audit| audit.original_content)
        .ok_or("the composition keeps the original content")?;
    let DvEncapsulated::DvParsable(parsable) = original else {
        return Err(format!("the original is no DV_PARSABLE: {original:?}").into());
    };
    assert_eq!(parsable.value, text, "the summary is kept byte for byte");
    Ok(())
}

// FHIR R4 Bundle references (<https://hl7.org/fhir/R4/bundle.html#references>): the summary
// names its one patient in one form, `Patient/{id}`, which resolves to the patient's entry.
#[test]
fn the_summary_names_its_patient_in_one_form() -> Result<(), Box<dyn Error>> {
    let bundle = document_mapped(&eu_mapping()?, 2)?;
    let text = serde_json::to_string(&bundle)?;
    let document = ReceivedDocument::read(&text)?;
    document.check(&eps(EPS_BUNDLE)?, &eps(EPS_COMPOSITION)?)?;
    let mut patients = Vec::new();
    let mut named = BTreeSet::new();
    for entry in &bundle.entry {
        let resource = entry.resource.as_ref().ok_or("an entry with no resource")?;
        if let Resource::Patient(patient) = resource {
            let id = patient.id.as_deref().ok_or("a patient with no id")?;
            patients.push(format!("Patient/{id}"));
        }
        let object = Json::to_json(resource)?;
        for key in ["subject", "patient"] {
            if let Some(Value::String(reference)) =
                object.get(key).and_then(|value| value.get("reference"))
            {
                named.insert(reference.clone());
            }
        }
    }
    let [patient] = patients.as_slice() else {
        return Err(format!("one Patient entry: {patients:?}").into());
    };
    assert_eq!(
        named,
        BTreeSet::from([patient.clone()]),
        "every subject and patient reference is the one form the patient entry is named by"
    );
    Ok(())
}
