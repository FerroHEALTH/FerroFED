// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The names of the `detail` entries a record's added entities carry.

/// A category as `<system>|<code>`, once per category.
pub const CATEGORY: &str = "ehds-category";
/// `<system>|<code>:<basis>`, once per basis of each category, the basis
/// after the last `:`.
pub const BASIS: &str = "ehds-category-basis";
/// `<system>|<version>`, once per code system a category is written in
/// whose version is known.
pub const CATEGORY_VERSION: &str = "ehds-category-version";
/// `true` when the access reached data of no category.
pub const NO_CATEGORY: &str = "ehds-no-category";
/// Why the access is unclassified.
pub const UNCLASSIFIED: &str = "ehds-unclassified";
/// A template id of the evidence.
pub const TEMPLATE_ID: &str = "template-id";
/// An archetype id of the evidence.
pub const ARCHETYPE_ID: &str = "archetype-id";
/// A version uid of a root object the access reached.
pub const VERSION_UID: &str = "version-uid";
/// An id the category map holds no key for.
pub const UNMAPPED_ID: &str = "unmapped-id";
/// The digest of the category map.
pub const MAP_DIGEST: &str = "category-map-digest";
/// The rows or objects delivered.
pub const DELIVERED: &str = "delivered";
/// The stored query executed.
pub const STORED_QUERY: &str = "stored-query";
/// The node behind an origin.
pub const NODE: &str = "node";
/// The `system_id` of the node behind an origin.
pub const SYSTEM_ID: &str = "system-id";
/// How an origin answered.
pub const STATUS: &str = "status";
/// The rows an origin contributed.
pub const ROWS: &str = "rows";
/// The `OBJECT_VERSION_ID` of the version a write left at an origin, as
/// its answer named it.
pub const WRITTEN_VERSION: &str = "written-version";
/// Why the version a write left at an origin cannot be told: `unnamed`
/// when its answer named none, `malformed` when it named no
/// `OBJECT_VERSION_ID`.
pub const WRITTEN_VERSION_UNREAD: &str = "written-version-unread";
/// The `ehr_id` of the EHR a create left at an origin, as its answer
/// named it.
pub const CREATED_EHR: &str = "created-ehr";
/// Why the `ehr_id` an EHR create left at an origin cannot be told:
/// `unnamed` when its answer named none, `malformed` when it named no
/// `HIER_OBJECT_ID`, or two that differ.
pub const CREATED_EHR_UNREAD: &str = "created-ehr-unread";
/// The endpoint an `ehr_id` is held at.
pub const ENDPOINT: &str = "endpoint";
/// Whether the patient behind an `ehr_id` is named: `request-named`,
/// `found`, `not-found`, `unavailable`, `not-configured` or
/// `unsupported`.
pub const PATIENT_LOOKUP: &str = "patient-lookup";
/// The identifier the client correlates the request by in its own log.
pub const CORRELATION: &str = "correlation-id";
/// The national contact point that asserted the relayed professional
/// and provider, by the issuer of its token.
pub const CONTACT_POINT: &str = "contact-point";
/// That the relayed values are the contact point's assertion: `true`.
pub const ASSERTED: &str = "asserted";
/// The relayed `country_code`.
pub const COUNTRY_CODE: &str = "country-code";
/// The relayed professional's `family_name`.
pub const FAMILY_NAME: &str = "hp-family-name";
/// The relayed professional's `given_name`.
pub const GIVEN_NAME: &str = "hp-given-name";
/// The relayed professional's `hp_identifier`.
pub const HP_IDENTIFIER: &str = "hp-identifier";
/// The agency that issued the relayed `hp_identifier`.
pub const HP_ISSUING_AUTHORITY: &str = "hp-issuing-authority";
/// `<system>|<code>`, or `<code>`, once per relayed
/// `hp_professional_role`.
pub const HP_ROLE: &str = "hp-professional-role";
/// The relayed `healthcare_provider_identifier`.
pub const PROVIDER_IDENTIFIER: &str = "provider-identifier";
/// The agency that issued the relayed provider identifier.
pub const PROVIDER_ISSUING_AUTHORITY: &str = "provider-issuing-authority";
/// The relayed `healthcare_provider_name`.
pub const PROVIDER_NAME: &str = "provider-name";
/// The relayed `healthcare_provider_address`.
pub const PROVIDER_ADDRESS: &str = "provider-address";
/// The years the record is kept.
pub const RETENTION_YEARS: &str = "ehds-retention-years";
/// The first UTC date the record may be deleted on, `YYYY-MM-DD`.
pub const RETENTION_ENDS: &str = "ehds-retention-ends";
/// What called for the period: `default`, `unclassified`,
/// `category:<system>|<code>` or `origin:<endpoint>`.
pub const RETENTION_GROUND: &str = "ehds-retention-ground";
/// That the access is an emergency access (Art 11(5)): `true`.
pub const EMERGENCY_ACCESS: &str = "ehds-emergency-access";
/// `<system>|<code>`, or `<code>`, once per purpose of use that marked
/// the access an emergency access.
pub const EMERGENCY_PURPOSE: &str = "ehds-emergency-purpose";
/// A member whose consent pre-filter denial was set aside for the
/// emergency, once per member.
pub const CONSENT_SET_ASIDE: &str = "ehds-consent-set-aside";
/// That the access is a population access: `true`.
pub const POPULATION: &str = "ehds-population";
/// Why a population access's record names no `ehr_id`: `not-projected`,
/// `over-limit`, `not-delivered` or `unattributed`.
pub const POPULATION_REASON: &str = "ehds-population-reason";
/// How many `ehr_id`s the delivered rows of a population access named.
pub const POPULATION_EHRS: &str = "ehds-population-ehrs";
/// The most `ehr_id`s a record names.
pub const POPULATION_LIMIT: &str = "ehds-population-limit";
/// Which way a semantic transformation ran: `provided` or `received`.
pub const TRANSFORMATION_DIRECTION: &str = "ehds-transformation-direction";
/// The Art 14(1) category of the data a transformation transformed, as
/// `<system>|<code>`.
pub const TRANSFORMATION_CATEGORY: &str = "ehds-transformation-category";
/// The FHIRconnect context mapping a transformation ran.
pub const MAPPING_CONTEXT: &str = "mapping-context";
/// The openEHR template id a transformation's mapping maps.
pub const MAPPING_TEMPLATE: &str = "mapping-template";
/// The profile a transformation's mapping maps to.
pub const MAPPING_PROFILE: &str = "mapping-profile";
/// A file name of a transformation's model or context mappings, once per
/// file.
pub const MAPPING_FILE: &str = "mapping-file";
/// The FHIR package a received document was held to.
pub const MAPPING_PACKAGE: &str = "mapping-package";
/// A terminology release or code system version in force, as
/// `<name>|<version>`, once per version.
pub const TERMINOLOGY_VERSION: &str = "terminology-version";
/// How many coded elements a transformation carried in a preferred code
/// system.
pub const TRANSCODED: &str = "ehds-transcoded";
/// `<path>=<count>`, once per path of a transcoded element.
pub const TRANSCODED_PATH: &str = "ehds-transcoded-path";
/// How many coded elements a transformation carried in no preferred code
/// system (eHealth Network Guidelines on Patient Summary Art 10(7)).
pub const UNTRANSCODED: &str = "ehds-untranscoded";
/// `<path>=<count>`, once per path of an untranscoded element.
pub const UNTRANSCODED_PATH: &str = "ehds-untranscoded-path";
/// How many elements a transformation did not carry.
pub const DROPPED: &str = "ehds-dropped";
/// `<path>=<count>`, once per path of a dropped element that names one.
pub const DROPPED_PATH: &str = "ehds-dropped-path";
