// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vaccination history fixtures (#1980).
//!
//! A synthetic composition of FerroBRIDGE's patient summary template whose
//! immunisations section records one dose of a vaccine, written in Dutch in
//! the Netherlands, and the `ORIGINAL_VERSION` a member answers for it, with
//! the attestations of the version. The vaccine carries a real SNOMED CT
//! code, since MyHealth@EU binds `Immunization.vaccineCode` required to
//! `eHDSIVaccine`; the dose, the attester and the record are invented.

use crate::medication::{dose, patient_summary, section};

/// The vaccine's SNOMED CT code, which MVC 10.1.0 `eHDSIVaccine` lists.
pub const VACCINE: &str = "836398006";

/// When the dose was given.
pub const GIVEN: &str = "2026-09-15T09:30:00Z";

/// When the synthetic attester attested the version.
pub const ATTESTED: &str = "2026-10-01T09:00:00Z";

/// The synthetic attester's name.
pub const ATTESTER: &str = "Synthetic Attester";

/// A dose of [`VACCINE`] administered at [`GIVEN`], its sequence number 1.
fn administered() -> String {
    let vaccine = format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "Streptococcus pneumoniae antigen-containing vaccine product", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://snomed.info/sct" }}, "code_string": "{VACCINE}" }} }}"#
    );
    dose(
        "Vaccination administered",
        &vaccine,
        &[String::from(
            r#"{ "_type": "ELEMENT", "name": { "_type": "DV_TEXT", "value": "Sequence number" }, "archetype_node_id": "at0025", "value": { "_type": "DV_COUNT", "magnitude": 1 } }"#,
        )],
    )
}

/// Returns a synthetic composition of [`PATIENT_SUMMARY_TEMPLATE`] under the
/// version id `uid`, in Dutch in the Netherlands, whose immunisations
/// section records one dose of [`VACCINE`].
///
/// [`PATIENT_SUMMARY_TEMPLATE`]: crate::medication::PATIENT_SUMMARY_TEMPLATE
#[must_use]
pub fn composition(uid: &str) -> String {
    patient_summary(
        uid,
        "nl",
        &section(
            "Immunisations",
            "openEHR-EHR-SECTION.immunisation_list.v0",
            &administered(),
        ),
    )
}

/// Returns the `ATTESTATION` of the version by [`ATTESTER`] at
/// [`ATTESTED`], in canonical JSON, pending when `pending` says so.
#[must_use]
pub fn attestation(pending: bool) -> String {
    format!(
        r#"{{
  "_type": "ATTESTATION",
  "system_id": "synthetic.example.org",
  "time_committed": {{ "_type": "DV_DATE_TIME", "value": "{ATTESTED}" }},
  "change_type": {{ "_type": "DV_CODED_TEXT", "value": "attestation", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "666" }} }},
  "committer": {{ "_type": "PARTY_IDENTIFIED", "name": "{ATTESTER}" }},
  "reason": {{ "_type": "DV_TEXT", "value": "Signed" }},
  "is_pending": {pending}
}}"#
    )
}

/// Returns the `ORIGINAL_VERSION` of the version `uid` of [`composition`],
/// in canonical JSON, with `attestations`, a JSON array of `ATTESTATION`s,
/// or with none.
#[must_use]
pub fn original_version(uid: &str, attestations: Option<&str>) -> String {
    let attestations = attestations
        .map(|attestations| format!(r#""attestations": {attestations},"#))
        .unwrap_or_default();
    format!(
        r#"{{
  "_type": "ORIGINAL_VERSION",
  "uid": {{ "_type": "OBJECT_VERSION_ID", "value": "{uid}" }},
  "contribution": {{ "id": {{ "_type": "HIER_OBJECT_ID", "value": "4f5c1d1e-0000-4000-8000-000000001980" }}, "namespace": "local", "type": "CONTRIBUTION" }},
  "commit_audit": {{
    "_type": "AUDIT_DETAILS",
    "system_id": "synthetic.example.org",
    "time_committed": {{ "_type": "DV_DATE_TIME", "value": "2026-10-01T08:00:00Z" }},
    "change_type": {{ "_type": "DV_CODED_TEXT", "value": "creation", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "249" }} }},
    "committer": {{ "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" }}
  }},
  "lifecycle_state": {{ "_type": "DV_CODED_TEXT", "value": "complete", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "532" }} }},
  {attestations}
  "data": {data}
}}"#,
        data = composition(uid),
    )
}
