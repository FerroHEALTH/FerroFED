// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document held, in the component, to every vendored
//! profile it claims: the `Bundle` to `bundle-eu-eps`, its `Composition` to
//! `composition-eu-eps`, and every entry to each profile its `meta.profile`
//! names, the EPS `patient-eu-eps` and the EU core profile a section's
//! mapping writes to included (FHIR R4 profiling,
//! <https://hl7.org/fhir/R4/profiling.html>). A claimed profile no vendored
//! package holds fails the test, so a claim is never left unchecked.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use eehrxf::document::{EPS_BUNDLE, EPS_PATIENT};
use eehrxf::mapping::Mapping;
use eehrxf::receive::conform::{CheckError, FindingKind, check_resource};
use fhir_types::codec::{Json, Value};
use fhir_types::r4::bundle::Bundle;

use super::document::{document_mapped, fixture};
use super::support::{eps, vendored};

/// The canonical URL of the EPS `Composition` profile.
const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The EU core profile the allergies section's mapping writes to.
const ALLERGY_EU_CORE: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core";

/// The synthetic mapping of the allergies section to the EU core profile the
/// patient summary crosswalk admits for it.
fn eu_mapping() -> Result<Mapping, Box<dyn Error>> {
    let opt = std::fs::read_to_string(fixture("opt/allergy.opt"))?;
    Ok(Mapping::compile(
        &opt,
        &[
            fixture("mapping/ferrofed_allergy.yml"),
            fixture("mapping/eu_allergy.context.yml"),
        ],
        &["ferrofed_eu_allergy.context"],
    )?)
}

/// The resource type `name` names, as the check takes it, for the types a
/// summary document writes.
fn resource_type(name: &str) -> Option<&'static str> {
    [
        "AllergyIntolerance",
        "Composition",
        "Device",
        "Organization",
        "Patient",
        "Provenance",
    ]
    .into_iter()
    .find(|known| *known == name)
}

/// Every `(profile, resource type)` the entries of `bundle` claim, each
/// checked against its vendored profile.
fn check_entries(bundle: &Bundle) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut claimed = BTreeSet::new();
    for entry in &bundle.entry {
        let resource = entry.resource.as_ref().ok_or("an entry with no resource")?;
        let object = Json::to_json(resource)?;
        let name = match object.get("resourceType") {
            Some(Value::String(name)) => name.clone(),
            _ => return Err("an entry with no resourceType".into()),
        };
        let profiles: Vec<String> = match object.get("meta").and_then(Value::as_object) {
            Some(meta) => match meta.get("profile") {
                Some(Value::Array(profiles)) => profiles
                    .iter()
                    .filter_map(|profile| match profile {
                        Value::String(url) => Some(url.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        for url in profiles {
            let kind =
                resource_type(&name).ok_or_else(|| format!("{name} claims {url} unchecked"))?;
            let profile = vendored(url.split('|').next().unwrap_or(&url))?;
            check_resource(&profile, kind, &object)
                .map_err(|refused| format!("{name} against {url}: {refused}"))?;
            claimed.insert(url);
        }
    }
    Ok(claimed)
}

#[test]
fn every_profile_the_document_claims_holds() -> Result<(), Box<dyn Error>> {
    let bundle = document_mapped(&eu_mapping()?, 1)?;
    let root = Json::to_json(&bundle)?;
    let provenance: BTreeSet<String> = bundle
        .entry
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            matches!(
                entry.resource,
                Some(fhir_types::r4::resource::Resource::Provenance(_))
            )
        })
        .map(|(index, _)| format!("Bundle.entry[{index}]"))
        .collect();
    match check_resource(&eps(EPS_BUNDLE)?, "Bundle", &root) {
        Ok(_) => {}
        // NOTE: FHIR R4 profiling: `bundle-eu-eps` slices `entry` open, so a Provenance entry
        // no slice names is admitted; the receive check holds it closed on type (#842).
        Err(CheckError::NonConformant { findings }) => {
            let left: Vec<_> = findings
                .iter()
                .filter(|finding| {
                    !(finding.kind == FindingKind::TypeNotAllowed
                        && provenance.contains(&finding.location))
                })
                .collect();
            assert!(left.is_empty(), "bundle-eu-eps: {left:?}");
        }
        Err(other) => return Err(other.into()),
    }
    let composition = bundle
        .entry
        .first()
        .and_then(|entry| entry.resource.as_ref())
        .ok_or("the composition entry")?;
    check_resource(
        &eps(EPS_COMPOSITION)?,
        "Composition",
        &Json::to_json(composition)?,
    )?;
    let claimed = check_entries(&bundle)?;
    assert!(
        claimed.contains(EPS_PATIENT),
        "the document claims {EPS_PATIENT}: {claimed:?}"
    );
    let allergies: Vec<_> = bundle
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
        .filter(|resource| {
            matches!(
                resource,
                fhir_types::r4::resource::Resource::AllergyIntolerance(_)
            )
        })
        .collect();
    assert_eq!(1, allergies.len(), "one mapped allergy");
    for allergy in allergies {
        // NOTE: the context mapping names the profile its output conforms to, which the entry
        // does not repeat in its meta.profile, so the entry is held to it here.
        check_resource(
            &vendored(ALLERGY_EU_CORE)?,
            "AllergyIntolerance",
            &Json::to_json(allergy)?,
        )?;
    }
    Ok(())
}

#[test]
fn an_entry_that_breaks_its_claimed_profile_is_found() -> Result<(), Box<dyn Error>> {
    let mut bundle = document_mapped(&eu_mapping()?, 1)?;
    let patient = bundle
        .entry
        .iter_mut()
        .filter_map(|entry| entry.resource.as_mut())
        .find_map(|resource| match resource {
            fhir_types::r4::resource::Resource::Patient(patient) => Some(patient),
            _ => None,
        })
        .ok_or("the patient entry")?;
    patient.birth_date = None;
    let object = Json::to_json(&fhir_types::r4::resource::Resource::Patient(
        patient.clone(),
    ))?;
    let refused = check_resource(&vendored(EPS_PATIENT)?, "Patient", &object);
    let Err(CheckError::NonConformant { findings }) = refused else {
        return Err(format!("a patient with no birth date passed: {refused:?}").into());
    };
    assert!(
        findings.iter().any(|finding| {
            finding.element == "Patient.birthDate"
                && matches!(finding.kind, FindingKind::TooFew { min: 1, found: 0 })
        }),
        "patient-eu-eps Patient.birthDate 1..1: {findings:?}"
    );
    Ok(())
}
