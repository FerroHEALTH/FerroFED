// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The preferred code systems of the eHealth Network Guidelines on Patient
//! Summary, by the system URI a FHIR coding carries.
//!
//! The guideline names the preferred code systems of each data element in
//! its §4 tables (Release 3.4, November 2024, Tables 2 and 3), by name, and
//! says they "are inspired by the eHealth Digital Service Infrastructure
//! implementation and the HL7 IPS implementations". Each constant here is
//! the URI by which the MyHealth@EU Master Value Sets Catalogue 9.1.0 (the
//! eHDSI implementation) includes that system in the value set it names, so
//! a coding a deployment sends is one MyHealth@EU reads. The catalogue
//! includes no NPU, so [`NPU`] is the URI HL7 Europe includes it by.
//!
//! Two names of §4 have no constant: EMDN, for which no vendored eHDSI or
//! HL7 package names a system URI, and IDMP, which §4 lists for use "when
//! available". A deployment that needs one adds it with
//! `PreferredSystems::new` (feature `openehr`).

/// The releases the preferred code systems are read from, each a name and
/// a version: the guideline's §4 tables and the catalogue that gives each
/// system's URI.
pub const RELEASES: [(&str, &str); 2] = [
    ("ehn-patient-summary-guidelines", "3.4"),
    ("myhealth-eu-mvc", "9.1.0"),
];

/// SNOMED CT, listed in §4 as "SNOMED CT GPS".
///
/// §4 lists it for the allergy, problem, procedure, device, vaccination,
/// social and pregnancy history and result elements (MVC 9.1.0
/// `eHDSIAllergenNoDrug`, `eHDSIProcedure` and others).
pub const SNOMED_CT: &str = "http://snomed.info/sct";

/// ICD-10, listed in §4 for the vaccination disease, problem and medication
/// reason elements (MVC 9.1.0 `eHDSIIllnessandDisorder`).
pub const ICD_10: &str = "http://hl7.org/fhir/sid/icd-10";

/// Orphacode, listed in §4 for a rare problem or medication reason (MVC
/// 9.1.0 `eHDSIRareDisease`).
pub const ORPHACODE: &str = "https://www.orpha.net";

/// ATC, listed in §4 for the allergy agent, vaccine and active ingredient
/// elements (MVC 9.1.0 `eHDSIActiveIngredient`).
pub const ATC: &str = "http://www.whocc.no/atc";

/// The EDQM Standard Terms, listed in §4 for the strength, pharmaceutical
/// dose form and route of administration elements (MVC 9.1.0
/// `eHDSIDoseForm`, `eHDSIRouteofAdministration`).
pub const EDQM_STANDARD_TERMS: &str = "http://standardterms.edqm.eu";

/// UCUM, listed in §4 for the strength and the units of a result (MVC 9.1.0
/// `eHDSIUnit`).
pub const UCUM: &str = "http://unitsofmeasure.org";

/// LOINC, listed in §4 for the result observation details (MVC 9.1.0
/// `eHDSILabCode`).
pub const LOINC: &str = "http://loinc.org";

/// NPU, listed in §4 beside LOINC for the result observation details (HL7
/// Europe Laboratory Report 2.0.0, `ValueSet` `lab-obsCode-npu-eu-lab`,
/// `compose.include.system`).
pub const NPU: &str = "http://npu-terminology.org";

/// ISO 3166, listed in §4 for the country of affiliation, the address
/// country, the country of vaccination and the travel history (MVC 9.1.0
/// `eHDSICountry`).
pub const ISO_3166: &str = "urn:iso:std:iso:3166";

/// HL7 Administrative Gender, listed in §4 for the patient's gender (MVC
/// 9.1.0 `eHDSIAdministrativeGender`).
pub const HL7_ADMINISTRATIVE_GENDER: &str =
    "http://terminology.hl7.org/CodeSystem/v3-AdministrativeGender";

/// HL7 `RoleClass`, listed in §4 for the role of a contact person (MVC 9.1.0
/// `eHDSIRoleClass`).
pub const HL7_ROLE_CLASS: &str = "http://terminology.hl7.org/CodeSystem/v3-RoleClass";

/// HL7 `RoleCode`, listed in §4 for a contact person's relationship to the
/// patient (MVC 9.1.0 `eHDSIPersonalRelationship`).
pub const HL7_ROLE_CODE: &str = "http://terminology.hl7.org/CodeSystem/v3-RoleCode";

/// HL7 `ObservationCategoryCodes`, listed in §4 for the observation type (MVC
/// 9.1.0 `eHDSIObservationCategory`).
pub const HL7_OBSERVATION_CATEGORY: &str =
    "http://terminology.hl7.org/CodeSystem/observation-category";

/// Every preferred code system of §4 this crate ships, in the order above.
pub const PATIENT_SUMMARY: [&str; 13] = [
    SNOMED_CT,
    ICD_10,
    ORPHACODE,
    ATC,
    EDQM_STANDARD_TERMS,
    UCUM,
    LOINC,
    NPU,
    ISO_3166,
    HL7_ADMINISTRATIVE_GENDER,
    HL7_ROLE_CLASS,
    HL7_ROLE_CODE,
    HL7_OBSERVATION_CATEGORY,
];
