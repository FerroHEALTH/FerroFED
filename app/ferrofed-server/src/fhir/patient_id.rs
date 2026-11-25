// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stable patient id of an answer, recorded in the store, and the
//! patient an id a caller sends back stands for.
//!
//! The id is derived from the identifier alone ([`crate::ids`]), so it is
//! the same in every answer whether or not the store records it. Recording
//! renews the row's window; an id the store does not hold, never recorded
//! or past `fhir.patient_id_lifetime`, resolves to no patient, which the
//! caller is told so it repeats the lookup. No specification governs this:
//! our own design.

use ferrofed_registry::definition::store::StoreError;

use crate::config::fhir::FhirSettings;
use crate::ids::{IdError, PatientId};
use crate::store::Store;

/// Why an id a caller sent could not be resolved.
#[derive(Debug, thiserror::Error)]
pub(super) enum Unresolved {
    /// The gateway runs with no store, so it keeps no id record.
    #[error("the gateway keeps no record of patient ids, so patient is not searched")]
    NoStore,
    /// The store did not answer.
    #[error("the store did not answer")]
    Store(#[source] StoreError),
    /// The recorded seal did not open under the key and the id.
    #[error("a patient id record did not open")]
    Open(#[source] IdError),
}

/// The id of the patient `system`|`value`, recorded in `store` for the
/// window `fhir` names; a record that fails is logged under `logged`, and
/// the id is answered still, since it is derived and not stored.
pub(super) async fn recorded(
    store: Option<&Store>,
    fhir: &FhirSettings,
    patient: (&str, &str),
    logged: &str,
) -> PatientId {
    let id = fhir.ids.patient(patient);
    let Some(store) = store else {
        return id;
    };
    let sealed = match fhir.ids.seal(&id, patient) {
        Ok(sealed) => sealed,
        Err(error) => {
            tracing::error!(request_id = logged, %error, "a patient id could not be sealed");
            return id;
        }
    };
    if let Err(failure) = store
        .record_patient_id(&id, sealed, fhir.patient_id_lifetime)
        .await
    {
        tracing::error!(
            request_id = logged,
            error = %crate::chain(&failure),
            "a patient id could not be recorded, so a search by it will be told to repeat the lookup"
        );
    }
    id
}

/// The patient `id` stands for, or `None` for an id the store does not
/// hold.
///
/// # Errors
///
/// [`Unresolved`] when the gateway has no store, the store does not answer
/// or the record does not open.
pub(super) async fn resolved(
    store: Option<&Store>,
    fhir: &FhirSettings,
    id: &PatientId,
) -> Result<Option<(String, String)>, Unresolved> {
    let store = store.ok_or(Unresolved::NoStore)?;
    let Some(sealed) = store.patient_id(id).await.map_err(Unresolved::Store)? else {
        return Ok(None);
    };
    fhir.ids
        .open(id, &sealed)
        .map(Some)
        .map_err(Unresolved::Open)
}
