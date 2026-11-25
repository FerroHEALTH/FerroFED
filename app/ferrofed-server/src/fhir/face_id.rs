// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stable ids of an answer, a patient's and its documents', recorded in
//! the store, and the patient an id a caller sends back stands for.
//!
//! Each id is derived ([`crate::ids`]), so it is the same in every answer
//! whether or not the store records it. Recording renews the row's window;
//! an id the store does not hold, never recorded or past `fhir.id_lifetime`,
//! resolves to no patient, and the caller is told to repeat the search that
//! gave it. No specification governs this: our own design.

use ferrofed_registry::definition::store::StoreError;

use crate::config::fhir::FhirSettings;
use crate::ids::{FaceId, IdError};
use crate::store::Store;

/// Why an id a caller sent could not be resolved.
#[derive(Debug, thiserror::Error)]
pub(super) enum Unresolved {
    /// The gateway runs with no store, so it keeps no id record.
    #[error(
        "the gateway keeps no record of the ids it gives, with no [store], so it resolves none"
    )]
    NoStore,
    /// The store did not answer.
    #[error("the store did not answer")]
    Store(#[source] StoreError),
    /// The recorded seal did not open under the key and the id.
    #[error("an id record did not open")]
    Open(#[source] IdError),
}

/// The id of the patient `system`|`value`, recorded in `store` for the
/// window `fhir` names ([`record`]).
pub(super) async fn recorded(
    store: Option<&Store>,
    fhir: &FhirSettings,
    patient: (&str, &str),
    logged: &str,
) -> FaceId {
    let id = fhir.ids.patient(patient);
    record(store, fhir, patient, vec![id.clone()], logged).await;
    id
}

/// Records each of `ids` as standing for the patient `system`|`value` in
/// `store`, for the window `fhir` names; a record that fails is logged under
/// `logged`, and the answer that names the ids is answered still, since they
/// are derived and not stored.
pub(super) async fn record(
    store: Option<&Store>,
    fhir: &FhirSettings,
    patient: (&str, &str),
    ids: Vec<FaceId>,
    logged: &str,
) {
    let Some(store) = store else {
        return;
    };
    let sealed: Result<Vec<(FaceId, Vec<u8>)>, IdError> = ids
        .into_iter()
        .map(|id| fhir.ids.seal(&id, patient).map(|sealed| (id, sealed)))
        .collect();
    let failure = match sealed {
        Ok(sealed) => store
            .record_face_ids(sealed, fhir.id_lifetime)
            .await
            .err()
            .map(|failure| crate::chain(&failure)),
        Err(error) => Some(error.to_string()),
    };
    if let Some(error) = failure {
        tracing::error!(
            request_id = logged,
            error,
            "the ids of an answer could not be recorded, so a request by one will be told to repeat the search"
        );
    }
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
    id: &FaceId,
) -> Result<Option<(String, String)>, Unresolved> {
    let store = store.ok_or(Unresolved::NoStore)?;
    let Some(sealed) = store.face_id(id).await.map_err(Unresolved::Store)? else {
        return Ok(None);
    };
    fhir.ids
        .open(id, &sealed)
        .map(Some)
        .map_err(Unresolved::Open)
}
