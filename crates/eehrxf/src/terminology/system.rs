// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Coding.system` an openEHR terminology id is written as
//! (feature `openehr`).
//!
//! FHIR R4 `Coding.system` is a `uri`, "the identification of the code
//! system that defines the meaning of the symbol in the code"
//! (<https://docs.rs/fhir-types/0.1.108/fhir_types/r4/coding/struct.Coding.html>).
//! openEHR names a terminology by a short identifier, "typically a value
//! like `"snomed_ct"` that is mapped elsewhere to the full URI identifying
//! the terminology" (openEHR BASE Release 1.1.0, Foundation Types,
//! `TERMINOLOGY_CODE.terminology_id`), taken from the US National Library
//! of Medicine UMLS list (openEHR RM Release 1.1.0, Support IM, Terminology
//! Package, Terminology Identifiers). [`fhir_system`] is that mapping:
//!
//! - `local` is the system of the archetype the code is defined in,
//!   [`archetype_local`];
//! - an identifier of [`OPENEHR_NAMES`] is the URI it names;
//! - an identifier that is itself an absolute URI is its own system;
//! - any other identifier has no known system, and no coding is written
//!   for it.
//!
//! The version part of a terminology id, "Version of this terminology, if
//! versioning supported, else the empty string" (openEHR BASE 1.1.0,
//! `TERMINOLOGY_ID.version_id`), is the FHIR `Coding.version`, "the version
//! of the code system which was used when choosing this code" (FHIR R4
//! `Coding.version`), only in the form the code system's versions take.
//! [`fhir_version`] is that rule.

use openehr_base::v1_3::base_types::definitions::definitions_impl::LOCAL_TERMINOLOGY_ID;
use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_base::v1_3::base_types::identification::lexical::IdError;
use url::Url;

use crate::terminology::ehn;

/// The base of the system URI of each archetype's local terminology, which
/// the archetype id completes.
///
/// No openEHR, HL7 or FHIRconnect text defines a URI for an archetype's
/// local codes, so this is a code system FerroFED owns, one per archetype:
/// an archetype id "denotes a single archetype", and two versions of an
/// archetype "are actually two distinct archetypes" (openEHR BASE Release
/// 1.1.0, Base Types, Archetype Identifiers), so two archetypes' `at0003`
/// are two concepts under two systems.
// NOTE: no specification governs this: our own design, a URI under a domain FerroFED owns.
pub const ARCHETYPE_LOCAL: &str = "https://ferrofed.eu/fhir/CodeSystem/archetype-local/";

/// The openEHR terminology identifiers this crate maps, each with the FHIR
/// system URI of [`ehn`] it names.
///
/// Each identifier is one the openEHR texts give: `SNOMED-CT` and `LOINC`
/// from the UMLS table of the Support IM, and `ICD10`, the WHO
/// classification, from the Identification Package of BASE 1.1.0, which
/// also says `ICD10AM` is a distinct terminology, so it is not mapped.
pub const OPENEHR_NAMES: [(&str, &str); 3] = [
    ("SNOMED-CT", ehn::SNOMED_CT),
    ("LOINC", ehn::LOINC),
    ("ICD10", ehn::ICD_10),
];

/// Returns the system URI of the local terminology of `archetype`.
///
/// # Errors
///
/// The [`IdError`] of an archetype id that is not one by the `archetype_id`
/// syntax of openEHR BASE, whose characters are letters, digits, `_`, `-`
/// and `.`, all of them unreserved in a URI.
pub fn archetype_local(archetype: &ArchetypeId) -> Result<String, IdError> {
    let checked: ArchetypeId = archetype.value.parse()?;
    Ok(format!("{ARCHETYPE_LOCAL}{}", checked.value))
}

/// Returns the FHIR system URI of the openEHR terminology named `name`, a
/// `local` code read in `archetype` when one is given.
///
/// `None` is a terminology with no known system: `local` with no archetype,
/// and an identifier neither [`OPENEHR_NAMES`] holds nor an absolute URI.
///
/// # Errors
///
/// The [`IdError`] of [`archetype_local`] for a `local` code read in an
/// archetype id that is not one.
pub fn fhir_system(name: &str, archetype: Option<&ArchetypeId>) -> Result<Option<String>, IdError> {
    if name == LOCAL_TERMINOLOGY_ID {
        return archetype.map(archetype_local).transpose();
    }
    if let Some((_, uri)) = OPENEHR_NAMES.iter().find(|(known, _)| *known == name) {
        return Ok(Some((*uri).to_owned()));
    }
    // NOTE: a failed parse is the answer here: the identifier is a name, not a URI of its own.
    Ok(Url::parse(name).is_ok().then(|| name.to_owned()))
}

/// The base of the URI a SNOMED CT version is written as, which an edition's
/// module id completes, optionally followed by `/version/` and a release.
///
/// The vendored HL7 packages write a SNOMED CT version this way: HL7 France
/// Core 2.2.0 includes SNOMED CT in its value sets at the version
/// `http://snomed.info/sct/900000000000207008/version/20260201`
/// (`ValueSet.compose.include.version`), and HL7 Europe Laboratory Report
/// 2.0.0 expands its value sets at `http://snomed.info/sct/900000000000207008`
/// (`Parameters` `exp-params`, `system-version`).
pub const SNOMED_CT_VERSION: &str = "http://snomed.info/sct/";

/// Returns the FHIR `Coding.version` of a code of `system` recorded under
/// the openEHR terminology version `version_id`, or `None` when none is
/// written.
///
/// - An empty `version_id`, a terminology with no version, writes none.
/// - For SNOMED CT ([`ehn::SNOMED_CT`]), a version is a URI naming an
///   edition ([`SNOMED_CT_VERSION`]). A `version_id` in that form is written
///   as recorded; any other, such as a release date alone, names no edition,
///   so no faithful translation exists and none is written.
/// - For the HL7 systems of [`ehn`], a version is dot-separated digits, as
///   the MyHealth@EU Master Value Sets Catalogue 9.1.0 includes them
///   (`4.0.0` in `eHDSIAdministrativeGender` and `eHDSIPersonalRelationship`,
///   `5.0.0` in `eHDSIRoleClass`, `2.0.0` in `eHDSIObservationCategory`). A
///   `version_id` in that form is written as recorded, and any other is not
///   one of the system's versions, so none is written.
/// - For any other system, no vendored text shows a version form, and
///   `version_id` is written as recorded.
#[must_use]
pub fn fhir_version<'a>(system: &str, version_id: &'a str) -> Option<&'a str> {
    if version_id.is_empty() {
        return None;
    }
    let in_form = match system {
        ehn::SNOMED_CT => is_snomed_ct_version(version_id),
        ehn::HL7_ADMINISTRATIVE_GENDER
        | ehn::HL7_ROLE_CLASS
        | ehn::HL7_ROLE_CODE
        | ehn::HL7_OBSERVATION_CATEGORY => is_dotted(version_id),
        _ => true,
    };
    in_form.then_some(version_id)
}

/// Returns whether `version` is a SNOMED CT version URI: an edition's module
/// id under [`SNOMED_CT_VERSION`], optionally followed by `/version/` and a
/// release.
fn is_snomed_ct_version(version: &str) -> bool {
    let Some(rest) = version.strip_prefix(SNOMED_CT_VERSION) else {
        return false;
    };
    match rest.split_once("/version/") {
        Some((module, release)) => is_digits(module) && is_digits(release),
        None => is_digits(rest),
    }
}

/// Returns whether `version` is digits separated by dots.
fn is_dotted(version: &str) -> bool {
    version.split('.').all(is_digits)
}

/// Returns whether `text` is one or more ASCII digits.
fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}
