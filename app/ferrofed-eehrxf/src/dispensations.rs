// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation received on the FHIR face (Regulation (EU) 2025/327 Art
//! 14(1)(c)), read for what routes it to the member that owns its
//! prescription.
//!
//! `POST {fhir-base}/MedicationDispense` is the FHIR R4 `create` interaction
//! (`http.html` §3.1.0.8) of one `MedicationDispense`. The text is held to
//! the HL7 Europe MPD 1.0.0 `MedicationDispense-eu-mpd` profile and each
//! contained `Medication` to `Medication-eu-mpd` by the component's receive
//! check, every constraint it cannot evaluate named, and read strictly as the
//! Xt-EHR `EHDSMedicationDispense` dataset. What is read of it here is the
//! patient, by the identifiers of `subject` (a logical reference, or a
//! contained `Patient` its local reference names), and each prescription it
//! was dispensed against (`authorizingPrescription`, the dataset's
//! `relatedRequest`), by its business identifier, beside which the literal
//! reference the face wrote may stand, and its own business identifier
//! (`MedicationDispense.identifier`), which the face reads the owner's
//! dispensations by before it writes, so a dispensation a client sends again
//! after a time-out is refused, never written twice (hazard I-12 of the
//! clinical safety risk file). The resource itself travels on as its
//! text, never rewritten, so a prescription named by the face's reference
//! alone, which the member cannot resolve, is refused. A withdrawal,
//! `PUT {fhir-base}/MedicationDispense/{id}` with `status`
//! `entered-in-error`, is read the same way, and travels on with its
//! top-level `id` alone replaced by the member's own ([`with_id`]), since
//! an update names its resource by the id of the server it reaches (FHIR R4
//! `http.html` §3.1.0.4). Which parts route it is our own design: no
//! specification governs it.

use eehrxf::medication::dispensation::Dispense;
use eehrxf::medication::{Mpd, ReadError};
use eehrxf::receive::conform::{CheckError, Finding, Unevaluated};
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::medication_dispense::MedicationDispense;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

/// The resource type received.
pub const RESOURCE: &str = "MedicationDispense";

/// One prescription a dispensation names: its business identifier, the
/// literal reference to it, or both, which must then name the same one.
///
/// `Debug` shows which of the two it carries, never a value.
#[derive(Clone, PartialEq, Eq)]
pub struct Named {
    /// The identifier's system, when it has one, and its value.
    pub identifier: Option<(Option<String>, String)>,
    /// The literal reference, as written.
    pub reference: Option<String>,
}

impl std::fmt::Debug for Named {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Named")
            .field("identifier", &self.identifier.is_some())
            .field("reference", &self.reference.is_some())
            .finish()
    }
}

/// A received dispensation, read for its routing.
///
/// `Debug` shows how many identifiers and prescriptions it names, and that it
/// carries a business identifier, never one.
#[derive(Clone)]
pub struct Received {
    /// Each identifier of the patient with both a system and a value.
    pub patient: Vec<(String, String)>,
    /// Each prescription it was dispensed against, in its order.
    pub named: Vec<Named>,
    /// Its business identifier, the first `identifier` with both a system
    /// and a value.
    pub identifier: (String, String),
    /// Every constraint of the profiles the check did not evaluate.
    pub unevaluated: Vec<Unevaluated>,
    /// Its logical id, which an update names it by (FHIR R4 `http.html`
    /// §3.1.0.4).
    pub id: Option<String>,
    /// Its `status`, a code of the FHIR R4 `medicationdispense-status`
    /// value set.
    pub status: Option<String>,
    /// The identifier of the facility that dispensed it: the first
    /// identifier with a system and a value of an `Organization` a
    /// `performer.actor` names, by a contained `Organization` or by its
    /// logical identifier on a reference typed `Organization`.
    pub dispenser: Option<(String, String)>,
}

impl std::fmt::Debug for Received {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Received")
            .field("patient", &self.patient.len())
            .field("named", &self.named.len())
            .field("identifier", &!self.identifier.1.is_empty())
            .field("unevaluated", &self.unevaluated.len())
            .field("id", &self.id.is_some())
            .field("status", &self.status)
            .field("dispenser", &self.dispenser.is_some())
            .finish()
    }
}

/// Why a received dispensation is refused before any member is asked; no
/// variant's text quotes a value of it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unread {
    /// The text is no `MedicationDispense` in FHIR JSON, or does not decode
    /// against FHIR R4 (`http.html` §3.1.0.8: `400`).
    #[error("the body is no FHIR R4 MedicationDispense in JSON: {0}")]
    Text(&'static str),
    /// The resource does not read through the R4 model.
    #[error("the body is no FHIR R4 MedicationDispense")]
    Decode(#[source] ReadError),
    /// The resource breaks its profiles (`http.html` §3.1.0.8: `422`).
    #[error("the dispensation breaks its profiles in {} places", findings.len())]
    NonConformant {
        /// Every finding, in walk order.
        findings: Vec<Finding>,
    },
    /// The profile check could not be run.
    #[error("the dispensation's profiles could not be checked")]
    Check(#[source] CheckError),
    /// The resource does not read as the dispensation dataset.
    #[error("the dispensation does not read as an EHDSMedicationDispense")]
    Dataset(#[source] ReadError),
    /// The dispensation names no prescription it was dispensed against.
    #[error("the dispensation names no prescription in authorizingPrescription")]
    NoPrescription,
    /// An `authorizingPrescription` carries neither an identifier with a
    /// value nor a literal reference.
    #[error(
        "an authorizingPrescription names its prescription by neither an identifier nor a reference"
    )]
    Unnamed,
    /// The dispensation carries no business identifier with both a system
    /// and a value, by which a dispensation sent again is told from a new
    /// one.
    #[error(
        "the dispensation carries no identifier with a system and a value in MedicationDispense.identifier, by which the gateway refuses one written twice"
    )]
    NoIdentifier,
    /// An `authorizingPrescription` names its prescription by a reference
    /// alone, which the member the dispensation reaches unchanged cannot
    /// resolve.
    #[error(
        "name the prescription by its identifier; the member cannot resolve the gateway's reference"
    )]
    ReferenceOnly,
}

impl Unread {
    /// Whether the refusal is of the text's form (`400`) rather than of its
    /// content (`422`), or the gateway's own failure.
    #[must_use]
    pub fn is_form(&self) -> bool {
        matches!(self, Self::Text(_) | Self::Decode(_))
    }
}

/// Reads `text`, a received `MedicationDispense`, holding it to `mpd`.
///
/// # Errors
///
/// The [`Unread`] that says why the dispensation is refused.
pub fn read(text: &str, mpd: &Mpd) -> Result<Received, Unread> {
    let value: Value =
        serde_json::from_str(text).map_err(|_unparsed| Unread::Text("the text is not JSON"))?;
    let Value::Object(object) = value else {
        return Err(Unread::Text("the text is not a JSON object"));
    };
    if object.get("resourceType").and_then(Value::as_str) != Some(RESOURCE) {
        return Err(Unread::Text("the resourceType is not MedicationDispense"));
    }
    let conformance = match mpd.check_dispense(&object) {
        Ok(conformance) => conformance,
        Err(CheckError::NonConformant { findings }) => {
            return Err(Unread::NonConformant { findings });
        }
        Err(CheckError::Unreadable { .. }) => {
            return Err(Unread::Text(
                "a coded element does not decode against FHIR R4",
            ));
        }
        Err(other) => return Err(Unread::Check(other)),
    };
    Dispense::read(text).map_err(|error| match error {
        ReadError::Json { .. }
        | ReadError::RepeatedName { .. }
        | ReadError::NotAnObject
        | ReadError::WrongType { .. }
        | ReadError::Decode { .. }
        | ReadError::Encode { .. }
        | ReadError::Unfaithful => Unread::Decode(error),
        other => Unread::Dataset(other),
    })?;
    let dispense = MedicationDispense::from_json(&object, &mut Path::root(RESOURCE))
        .map_err(|_undecoded| Unread::Text("the resource does not decode against FHIR R4"))?;
    let named = dispense
        .authorizing_prescription
        .iter()
        .map(named)
        .collect::<Result<Vec<_>, _>>()?;
    if named.is_empty() {
        return Err(Unread::NoPrescription);
    }
    // NOTE: Federation Tier §5.4 scope note, N33, CP-26: the body reaches its member unchanged,
    // so a prescription is named by the identifier the member holds, never by the face's id.
    if named.iter().any(|one| one.identifier.is_none()) {
        return Err(Unread::ReferenceOnly);
    }
    // NOTE: no specification governs this: our own design; MPD 1.0.0 leaves the identifier
    // optional, and without it a dispensation sent again cannot be told from a new one.
    let identifier = dispense
        .identifier
        .iter()
        .find_map(pair)
        .ok_or(Unread::NoIdentifier)?;
    Ok(Received {
        patient: dispense
            .subject
            .as_ref()
            .map(|subject| patient(subject, &dispense.contained))
            .unwrap_or_default(),
        named,
        identifier,
        unevaluated: conformance.unevaluated().to_vec(),
        id: dispense.id.clone(),
        status: dispense.status.value.clone(),
        dispenser: dispenser(&dispense),
    })
}

/// The top-level members of a JSON object, each value as its own text.
struct Members<'a>(Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Members<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// The visitor that keeps each member in its order.
        struct Visit;

        impl<'de> Visitor<'de> for Visit {
            type Value = Members<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry::<String, &'de RawValue>()? {
                    members.push(member);
                }
                Ok(Members(members))
            }
        }

        deserializer.deserialize_map(Visit)
    }
}

/// `text`, a received resource, with the value of its top-level `id`
/// replaced by `id` and every other byte as it was; `None` when `text` is
/// no JSON object with a top-level `id`.
///
/// The face names a member's resource by its own id and the member by the
/// member's, so an update reaches the member under the member's id (FHIR
/// R4 `http.html` §3.1.0.4), the inverse of the rename a read makes
/// (§3.1.0.2).
#[must_use]
pub fn with_id(text: &str, id: &str) -> Option<String> {
    // NOTE: a text that is no object with an `id` has no id to replace, which the caller
    // checked before it asks, so its absence is the answer and never a second error.
    let members: Members<'_> = serde_json::from_str(text).ok()?;
    let held = members.0.iter().find(|(name, _)| name == "id")?.1.get();
    let start = held.as_ptr().addr().checked_sub(text.as_ptr().addr())?;
    let end = start.checked_add(held.len())?;
    let replacement = serde_json::to_string(id).ok()?;
    Some(format!(
        "{}{replacement}{}",
        text.get(..start)?,
        text.get(end..)?
    ))
}

/// The prescription `reference` names.
fn named(reference: &Reference) -> Result<Named, Unread> {
    let identifier = reference.identifier.as_deref().and_then(|identifier| {
        let value = identifier.value.as_ref()?.value.clone()?;
        let system = identifier
            .system
            .as_ref()
            .and_then(|system| system.value.clone());
        Some((system, value))
    });
    let literal = reference
        .reference
        .as_ref()
        .and_then(|text| text.value.clone());
    if identifier.is_none() && literal.is_none() {
        return Err(Unread::Unnamed);
    }
    Ok(Named {
        identifier,
        reference: literal,
    })
}

/// The identifiers of the patient `subject` names: its own logical
/// identifier, or those of the contained `Patient` its local reference
/// names; none for a reference the face cannot read a patient from.
fn patient(subject: &Reference, contained: &[Resource]) -> Vec<(String, String)> {
    let local = subject
        .reference
        .as_ref()
        .and_then(|text| text.value.as_deref())
        .and_then(|text| text.strip_prefix('#'));
    if let Some(local) = local {
        return contained
            .iter()
            .find_map(|resource| match resource {
                Resource::Patient(patient) if patient.id.as_deref() == Some(local) => {
                    Some(patient.identifier.iter().filter_map(pair).collect())
                }
                _ => None,
            })
            .unwrap_or_default();
    }
    subject
        .identifier
        .as_deref()
        .and_then(pair)
        .into_iter()
        .collect()
}

/// The identifier of the organisation that dispensed `dispense`, when a
/// `performer.actor` names an `Organization` with one.
fn dispenser(dispense: &MedicationDispense) -> Option<(String, String)> {
    // NOTE: HL7 Europe MPD 1.0.0 `MedicationDispense-eu-mpd`: `performer.actor` "should be
    // assumed" the dispenser, the element RC 07.01's facility identifier is read from.
    dispense.performer.iter().find_map(|performer| {
        let actor = &performer.actor;
        let local = actor
            .reference
            .as_ref()
            .and_then(|text| text.value.as_deref())
            .and_then(|text| text.strip_prefix('#'));
        if let Some(local) = local {
            return dispense
                .contained
                .iter()
                .find_map(|resource| match resource {
                    Resource::Organization(organization)
                        if organization.id.as_deref() == Some(local) =>
                    {
                        organization.identifier.iter().find_map(pair)
                    }
                    _ => None,
                });
        }
        let typed = actor
            .r#type
            .as_ref()
            .and_then(|uri| uri.value.as_deref())
            .is_some_and(|uri| uri == "Organization");
        typed
            .then(|| actor.identifier.as_deref().and_then(pair))
            .flatten()
    })
}

/// The system and the value of `identifier`, when it has both.
fn pair(identifier: &Identifier) -> Option<(String, String)> {
    let system = identifier.system.as_ref()?.value.clone()?;
    let value = identifier.value.as_ref()?.value.clone()?;
    (!system.is_empty() && !value.is_empty()).then_some((system, value))
}

/// The `OperationOutcome` of a dispensation the member `member` created.
///
/// It holds one `information` issue naming the member and its endpoint
/// `endpoint`, one `warning` per constraint of `received`'s profiles the check did not
/// evaluate, and `unlocated` as a further `warning` when the face can name
/// no location of it.
#[must_use]
pub fn written(
    received: &Received,
    (member, endpoint): (&str, &str),
    unlocated: Option<&str>,
) -> OperationOutcome {
    let mut outcome = told(
        received,
        &format!(
            "the Electronic-Dispensations dispensation was written to member {member} through endpoint {endpoint}"
        ),
    );
    if let Some(unlocated) = unlocated {
        outcome
            .issue
            .push(issue(("warning", "incomplete"), unlocated, None));
    }
    outcome
}

/// The `OperationOutcome` of a dispensation the member `member` withdrew,
/// as [`written`] tells a receipt, naming the member and its endpoint
/// `endpoint`.
#[must_use]
pub fn withdrawn(received: &Received, (member, endpoint): (&str, &str)) -> OperationOutcome {
    told(
        received,
        &format!(
            "the Electronic-Dispensations dispensation was withdrawn at member {member} through endpoint {endpoint}"
        ),
    )
}

/// One `information` issue saying `what` happened to `received`, and one
/// `warning` per constraint of its profiles the check did not evaluate.
fn told(received: &Received, what: &str) -> OperationOutcome {
    let mut issues = vec![issue(("information", "informational"), what, None)];
    issues.extend(received.unevaluated.iter().map(|unevaluated| {
        issue(
            ("warning", "not-supported"),
            &format!(
                "{} of {} was not evaluated",
                unevaluated.reason, unevaluated.profile
            ),
            Some(&unevaluated.element),
        )
    }));
    OperationOutcome {
        issue: issues,
        ..OperationOutcome::default()
    }
}

/// One issue of `severity` and the FHIR R4 `issue-type` `kind`, with the
/// text `diagnostics`, at the element path `at` when one is named.
fn issue(
    (severity, kind): (&str, &str),
    diagnostics: &str,
    at: Option<&str>,
) -> OperationOutcomeIssue {
    OperationOutcomeIssue {
        severity: severity.into(),
        code: kind.into(),
        diagnostics: Some(diagnostics.into()),
        expression: at.map(Into::into).into_iter().collect(),
        ..OperationOutcomeIssue::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Unread, read, with_id};
    use eehrxf::medication::Mpd;

    fn mpd() -> Mpd {
        let archive = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/specs/eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz"),
        )
        .unwrap();
        Mpd::read(&archive).unwrap()
    }

    const DISPENSE: &str = r##"{"resourceType":"MedicationDispense","meta":{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd"]},
      "extension":[{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-10-16T10:00:00Z"}],
      "contained":[{"resourceType":"Patient","id":"pat","identifier":[{"system":"urn:oid:2.999.1","value":"SYNTHETIC-1"}]}],
      "identifier":[{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-1"}],
      "status":"completed","medicationCodeableConcept":{"text":"Synthetic medicine A"},
      "subject":{"reference":"#pat"},
      "performer":[{"actor":{"display":"Synthetic pharmacist"}}],
      "authorizingPrescription":[{"identifier":{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}}],
      "quantity":{"value":30,"unit":"tablet"}}"##;

    #[test]
    fn a_dispensation_names_its_patient_and_its_prescription() {
        let received = read(DISPENSE, &mpd()).unwrap();
        assert_eq!(
            received.identifier,
            ("urn:oid:2.999.6.1".to_owned(), "SYNTHETIC-MD-1".to_owned())
        );
        assert_eq!(
            received.patient,
            [("urn:oid:2.999.1".to_owned(), "SYNTHETIC-1".to_owned())]
        );
        let [named] = received.named.as_slice() else {
            panic!("one prescription: {received:?}");
        };
        assert_eq!(
            named.identifier,
            Some((
                Some("urn:oid:2.999.5.1".to_owned()),
                "SYNTHETIC-RX-A1".to_owned()
            ))
        );
        assert!(!format!("{received:?}").contains("SYNTHETIC"));
    }

    #[test]
    fn a_dispensation_carries_its_id_and_status() {
        let text = DISPENSE.replace(
            r#"{"resourceType":"MedicationDispense","#,
            r#"{"resourceType":"MedicationDispense","id":"face-id","#,
        );
        let received = read(&text, &mpd()).unwrap();
        assert_eq!(received.id.as_deref(), Some("face-id"));
        assert_eq!(received.status.as_deref(), Some("completed"));
    }

    #[test]
    fn the_dispenser_is_the_organization_a_performer_names() {
        assert_eq!(read(DISPENSE, &mpd()).unwrap().dispenser, None);
        let contained = DISPENSE
            .replace(
                r#""contained":[{"resourceType":"Patient","#,
                r#""contained":[{"resourceType":"Organization","id":"pharmacy","identifier":[{"system":"urn:oid:2.999.8","value":"SYNTHETIC-PHARMACY-1"}]},{"resourceType":"Patient","#,
            )
            .replace(
                r#"{"actor":{"display":"Synthetic pharmacist"}}"#,
                r##"{"actor":{"display":"Synthetic pharmacist"}},{"actor":{"reference":"#pharmacy"}}"##,
            );
        let expected = Some((
            "urn:oid:2.999.8".to_owned(),
            "SYNTHETIC-PHARMACY-1".to_owned(),
        ));
        let received = read(&contained, &mpd()).unwrap();
        assert_eq!(received.dispenser, expected);
        assert!(!format!("{received:?}").contains("PHARMACY"));
        let logical = DISPENSE.replace(
            r#"{"actor":{"display":"Synthetic pharmacist"}}"#,
            r#"{"actor":{"type":"Organization","identifier":{"system":"urn:oid:2.999.8","value":"SYNTHETIC-PHARMACY-1"}}}"#,
        );
        assert_eq!(read(&logical, &mpd()).unwrap().dispenser, expected);
        let practitioner = DISPENSE.replace(
            r#"{"actor":{"display":"Synthetic pharmacist"}}"#,
            r#"{"actor":{"type":"Practitioner","identifier":{"system":"urn:oid:2.999.8","value":"SYNTHETIC-PHARMACIST-1"}}}"#,
        );
        assert_eq!(read(&practitioner, &mpd()).unwrap().dispenser, None);
    }

    #[test]
    fn the_id_alone_is_replaced_and_every_other_byte_kept() {
        let text = "{ \"resourceType\" : \"MedicationDispense\",\n  \"id\" :  \"face-id\" ,\"note\":[{\"text\":\"id \\\"face-id\\\" é\"}],\"contained\":[{\"id\":\"face-id\"}]}\n";
        let swapped = with_id(text, "member-1").unwrap();
        assert_eq!(swapped, text.replacen("\"face-id\" ,", "\"member-1\" ,", 1));
        assert!(swapped.contains("\"contained\":[{\"id\":\"face-id\"}]"));
    }

    #[test]
    fn a_text_with_no_top_level_id_has_none_to_replace() {
        assert_eq!(with_id(r#"{"contained":[{"id":"a"}]}"#, "b"), None);
        assert_eq!(with_id("[]", "b"), None);
        assert_eq!(with_id("{", "b"), None);
    }

    #[test]
    fn a_text_that_is_no_dispensation_is_refused_by_its_form() {
        for text in ["[]", "{", r#"{"resourceType":"MedicationRequest"}"#] {
            assert!(read(text, &mpd()).unwrap_err().is_form(), "{text}");
        }
    }

    #[test]
    fn a_dispensation_with_no_business_identifier_is_refused() {
        let unidentified = DISPENSE.replace(
            r#""identifier":[{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-1"}],"#,
            "",
        );
        assert!(matches!(
            read(&unidentified, &mpd()),
            Err(Unread::NoIdentifier)
        ));
        let no_system = DISPENSE.replace(
            r#"{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-1"}"#,
            r#"{"value":"SYNTHETIC-MD-1"}"#,
        );
        assert!(matches!(
            read(&no_system, &mpd()),
            Err(Unread::NoIdentifier)
        ));
    }

    #[test]
    fn a_dispensation_naming_no_prescription_is_refused() {
        let text = DISPENSE.replace(
            r#""authorizingPrescription":[{"identifier":{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}}],"#,
            "",
        );
        assert!(matches!(
            read(&text, &mpd()),
            Err(Unread::NoPrescription | Unread::NonConformant { .. })
        ));
    }
}
