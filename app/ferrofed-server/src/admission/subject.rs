// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic subjects an admission check creates its test EHRs for.
//!
//! Each subject is minted fresh for one run: its namespace is
//! [`NAMESPACE`], inside the example arc ITU-T X.660 and ISO/IEC 9834 reserve
//! for examples, and its value is `ffd-admission-` followed by the 32 hex
//! digits of a random UUID, which no national identifier scheme validates.
//! The value is held as a [`SecretString`] like any patient identifier
//! (§5.4, N33): it travels only in the `EHR_STATUS` of a create, which is a
//! write body (§5.4 scope note), and to the configured cross-reference.

use std::fmt;

use ferrofed_identity::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrofed_registry::secret::REDACTED;
use openehr_base::v1_3::base_types::identification::archetype_id::ArchetypeId;
use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_rm::v1_2::common::archetyped::archetyped::Archetyped;
use openehr_rm::v1_2::common::generic::party_self::PartySelf;
use openehr_rm::v1_2::data_types::text::dv_text::{DvText, DvTextData};
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

/// The namespace of every synthetic subject: an arc of the example OID
/// `2.999` (ITU-T X.660, ISO/IEC 9834).
pub const NAMESPACE: &str = "urn:oid:2.999.1.0";

/// The prefix of every synthetic subject's value.
pub const VALUE_PREFIX: &str = "ffd-admission-";

/// The archetype of the `EHR_STATUS` a test EHR is created with.
const EHR_STATUS_ARCHETYPE: &str = "openEHR-EHR-EHR_STATUS.generic.v1";

/// One synthetic subject, minted fresh for one run.
///
/// `Debug` never shows the value.
pub struct SyntheticSubject {
    value: SecretString,
}

impl SyntheticSubject {
    /// Mints a subject no earlier run and no real patient has.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            value: SecretString::from(format!("{VALUE_PREFIX}{}", Uuid::new_v4().simple())),
        }
    }

    /// The subject's value, which no request to a node may carry outside a
    /// write body.
    #[must_use]
    pub fn value(&self) -> &SecretString {
        &self.value
    }

    /// The subject as a patient reference, for the cross-reference.
    ///
    /// # Errors
    ///
    /// Returns [`PatientRefError`] when the namespace or the value is empty,
    /// which neither ever is.
    pub fn patient(&self) -> Result<PatientRef, PatientRefError> {
        PatientRef::new(IdentifierNamespace::new(NAMESPACE)?, self.value.clone())
    }

    /// The `EHR_STATUS` a test EHR is created with: a queryable, modifiable
    /// status whose `PARTY_SELF` subject refers to this subject
    /// ([`ehr_status`]).
    #[must_use]
    pub fn ehr_status(&self) -> EhrStatus {
        ehr_status(Some(PartyRef {
            namespace: NAMESPACE.to_owned(),
            r#type: "PERSON".to_owned(),
            id: ObjectId::GenericId(GenericId {
                value: self.value.expose_secret().to_owned(),
                scheme: "ffd-admission".to_owned(),
            }),
        }))
    }
}

/// Returns the `EHR_STATUS` a synthetic EHR is created with: a queryable,
/// modifiable status whose `PARTY_SELF` subject refers to `external_ref`, or
/// an anonymous `PARTY_SELF` when there is none.
///
/// The status names its archetype (RM `LOCATABLE` invariant
/// `Archetyped_valid`), as an archetype root must.
#[must_use]
pub fn ehr_status(external_ref: Option<PartyRef>) -> EhrStatus {
    EhrStatus {
        name: DvText::DvText(DvTextData {
            value: "EHR Status".to_owned(),
            hyperlink: None,
            formatting: None,
            mappings: None,
            language: None,
            encoding: None,
        }),
        archetype_node_id: EHR_STATUS_ARCHETYPE.to_owned(),
        uid: None,
        links: None,
        archetype_details: Some(Archetyped {
            archetype_id: ArchetypeId {
                value: EHR_STATUS_ARCHETYPE.to_owned(),
            },
            template_id: None,
            rm_version: "1.1.0".to_owned(),
        }),
        feeder_audit: None,
        subject: PartySelf { external_ref },
        is_queryable: true,
        is_modifiable: true,
        other_details: None,
    }
}

impl fmt::Debug for SyntheticSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntheticSubject")
            .field("namespace", &NAMESPACE)
            .field("value", &REDACTED)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{NAMESPACE, SyntheticSubject, VALUE_PREFIX};
    use secrecy::ExposeSecret;

    #[test]
    fn every_subject_is_fresh_and_in_the_example_arc() {
        let first = SyntheticSubject::fresh();
        let second = SyntheticSubject::fresh();
        assert_ne!(
            first.value().expose_secret(),
            second.value().expose_secret()
        );
        assert!(first.value().expose_secret().starts_with(VALUE_PREFIX));
        assert!(NAMESPACE.starts_with("urn:oid:2.999."));
        let patient = first.patient().expect("the subject is a patient reference");
        assert_eq!(NAMESPACE, patient.namespace().as_str());
    }

    #[test]
    fn debug_never_shows_the_value() {
        let subject = SyntheticSubject::fresh();
        let shown = format!("{subject:?}");
        assert!(!shown.contains(subject.value().expose_secret()), "{shown}");
        assert!(shown.contains(r#"value: "***""#), "{shown}");
    }
}
