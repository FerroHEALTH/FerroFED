// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 definitions of the extensions and identifier systems the
//! registry document in FHIR form uses, each as the JSON the documentation
//! site serves at its `ferrofed.eu` URL.
//!
//! The extensions are defined after FHIR R4 2.5.1 Defining Extensions, and
//! each identifier system is a FHIR R4 `NamingSystem` whose preferred `uri`
//! is the system. The reader of the form ([`super::snapshot_from_json`]) is
//! the code that gives them their meaning, so they are kept beside it (no
//! specification governs where a definition lives: our own design).

/// The `StructureDefinition` of [`super::CONSENT_REFUSAL_CODE_EXTENSION`]:
/// an extension of `Endpoint`, any number of times, with a `valueCode`.
pub const CONSENT_REFUSAL_CODE: &str =
    include_str!("definition/StructureDefinition-consent-refusal-code.json");

/// The `StructureDefinition` of [`super::PROFILE_EXTENSION`]: an extension
/// of `Endpoint`, any number of times, with a `valueCanonical`.
pub const PAYLOAD_PROFILE: &str =
    include_str!("definition/StructureDefinition-payload-profile.json");

/// The `StructureDefinition` of [`super::EHR_ID_SYSTEM_EXTENSION`]: an
/// extension of `Endpoint`, at most once, with a `valueUri`.
pub const EHR_ID_SYSTEM: &str = include_str!("definition/StructureDefinition-ehr-id-system.json");

/// The `NamingSystem` of [`super::ORGANISATION_ID_SYSTEM`].
pub const ORGANISATION_ID: &str = include_str!("definition/NamingSystem-organisation-id.json");

/// The `NamingSystem` of [`super::ENDPOINT_ID_SYSTEM`].
pub const ENDPOINT_ID: &str = include_str!("definition/NamingSystem-endpoint-id.json");

/// The `NamingSystem` of [`super::NODE_ID_SYSTEM`].
pub const NODE_ID: &str = include_str!("definition/NamingSystem-node-id.json");

/// The `NamingSystem` of [`super::SYSTEM_ID_SYSTEM`].
pub const SYSTEM_ID: &str = include_str!("definition/NamingSystem-system-id.json");

/// The `NamingSystem` of [`super::CREATING_SYSTEM_ID_SYSTEM`].
pub const CREATING_SYSTEM_ID: &str =
    include_str!("definition/NamingSystem-creating-system-id.json");
