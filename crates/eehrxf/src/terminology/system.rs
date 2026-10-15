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
