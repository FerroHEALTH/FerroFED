// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received patient summary validated in full by FerroBRIDGE's validator
//! over the HL7 Europe Patient Summary and its whole dependency closure
//! (#808, FHIR R4 validation.html and profiling.html): a conforming document
//! is prepared with every constraint the validator did not evaluate named,
//! and a document that breaks an invariant, a required binding or the
//! profile an entry claims is refused naming where, before any mapping.

use std::error::Error;

use eehrxf::receive::conform::{Finding, FindingKind, Unread};
use eehrxf::terminology::Terminology;
use ferrobridge_validator::set::LoadError;
use ferrofed_eehrxf::receive::{Prepared, Receivers, ReceiversError, Source, Unreceived};
use ferrofed_testkit::{eps, fhir_packages};
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// The synthetic patient's namespace, under the example OID arc.
const NAMESPACE: &str = "urn:oid:2.999.1";

/// The synthetic patient.
const PATIENT: &str = "SENTINEL-VALIDATE-808";

/// The receivers of a deployment that takes patient summaries at `node-a`.
fn receivers() -> Result<Receivers, Box<dyn Error>> {
    let [model, context] = eps::mapping_files();
    Ok(Receivers::compile(&[Source {
        category: String::from("Patient-Summaries"),
        member: String::from("node-a"),
        package: fhir_packages::patient_summary(),
        dependencies: fhir_packages::dependencies()?,
        opt: std::fs::read_to_string(eps::opt())?,
        files: vec![model, context],
        context: String::from(eps::CONTEXT),
        language: String::from("en"),
        territory: String::from("NL"),
    }])?)
}

/// The synthetic document, changed by `change` at its allergy entry.
fn document(
    change: impl FnOnce(&mut serde_json::Map<String, Value>),
) -> Result<String, Box<dyn Error>> {
    let mut document: Value = serde_json::from_str(&eps::received_document(
        &[(NAMESPACE, PATIENT)],
        "Synthetic substance",
    ))?;
    let allergy = document
        .pointer_mut("/entry/2/resource")
        .and_then(Value::as_object_mut)
        .ok_or("the fixture's third entry is its allergy")?;
    change(allergy);
    Ok(document.to_string())
}

/// `text` received with no terminology server.
async fn receive(text: &str) -> Result<Result<Prepared, Unreceived>, Box<dyn Error>> {
    Ok(receivers()?
        .receive(
            text,
            ("Device/gateway", "2026-10-24T00:00:00Z"),
            (&Terminology::none(), None),
        )
        .await)
}

/// The findings of a refused document.
fn findings(outcome: Result<Prepared, Unreceived>) -> Result<Vec<Finding>, Box<dyn Error>> {
    match outcome {
        Err(Unreceived::NonConformant { findings }) => Ok(findings),
        Ok(_) => Err("the document was prepared".into()),
        Err(other) => Err(format!("the document was refused otherwise: {other}").into()),
    }
}

/// Whether `finding` is an error the validator reported.
fn is_error(finding: &Finding) -> bool {
    matches!(finding.kind, FindingKind::Issue { severity, .. } if severity == "error" || severity == "fatal")
}

#[tokio::test]
async fn a_conforming_summary_is_prepared_with_every_constraint_not_evaluated_named() -> TestResult
{
    let prepared = receive(&document(|_| {})?).await??;
    assert!(
        !prepared.unevaluated.is_empty(),
        "the validator names what it did not evaluate"
    );
    assert!(
        prepared
            .unevaluated
            .iter()
            .all(|unevaluated| matches!(unevaluated.reason, Unread::Constraint { .. })),
        "{:?}",
        prepared.unevaluated
    );
    assert!(
        prepared.cautions.iter().all(|caution| !is_error(caution)),
        "a caution is never an error: {:?}",
        prepared.cautions
    );
    Ok(())
}

#[tokio::test]
async fn a_document_that_breaks_an_invariant_is_refused_naming_it() -> TestResult {
    let findings = findings(
        receive(&document(|allergy| {
            allergy.remove("clinicalStatus");
        })?)
        .await?,
    )?;
    let broken = findings
        .iter()
        .find(|finding| {
            finding.element == "ait-1" && finding.location == "Bundle.entry[2].resource"
        })
        .ok_or_else(|| format!("ait-1 is found on the allergy: {findings:#?}"))?;
    assert_eq!(
        "http://hl7.org/fhir/StructureDefinition/AllergyIntolerance", broken.profile,
        "the finding names the definition that declares the invariant"
    );
    assert!(is_error(broken));
    Ok(())
}

#[tokio::test]
async fn a_concept_outside_a_required_binding_is_refused() -> TestResult {
    let findings = findings(
        receive(&document(|allergy| {
            allergy.insert(
                String::from("clinicalStatus"),
                serde_json::json!({ "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                    "code": "synthetic-not-a-status",
                    "display": "Synthetic"
                }] }),
            );
        })?)
        .await?,
    )?;
    assert!(
        findings.iter().any(|finding| is_error(finding)
            && finding
                .location
                .starts_with("Bundle.entry[2].resource.clinicalStatus")),
        "the clinical status outside its required value set is found: {findings:#?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_entry_that_breaks_the_profile_it_claims_is_refused() -> TestResult {
    let findings = findings(
        receive(&document(|allergy| {
            allergy.remove("code");
        })?)
        .await?,
    )?;
    assert!(
        findings.iter().any(|finding| is_error(finding)
            && finding.element == "claimed-profile-mismatch"
            && finding.location == "Bundle.entry[2].resource"),
        "the allergy is held to allergyIntolerance-eu-core, which it claims: {findings:#?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_package_set_short_of_a_dependency_is_refused_naming_it() -> TestResult {
    let [model, context] = eps::mapping_files();
    let refused = Receivers::compile(&[Source {
        category: String::from("Patient-Summaries"),
        member: String::from("node-a"),
        package: fhir_packages::patient_summary(),
        dependencies: Vec::new(),
        opt: std::fs::read_to_string(eps::opt())?,
        files: vec![model, context],
        context: String::from(eps::CONTEXT),
        language: String::from("en"),
        territory: String::from("NL"),
    }]);
    let Err(error) = refused else {
        return Err("a package with none of its dependencies is refused".into());
    };
    assert!(
        matches!(
            &error,
            ReceiversError::Packages {
                source,
                ..
            } if matches!(&**source, LoadError::MissingDependency { package, .. } if package == "hl7.fhir.eu.eps")
        ),
        "the dependency the Patient Summary names is missing: {error:?}"
    );
    let missing = Error::source(&error).map(ToString::to_string);
    assert!(
        missing
            .as_deref()
            .is_some_and(|text| text.contains("hl7.fhir.eu.eps")),
        "the refusal names the package: {missing:?}"
    );
    Ok(())
}
