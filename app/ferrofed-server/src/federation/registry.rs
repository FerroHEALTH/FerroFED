// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry a federation is built over: the document or the care
//! services directory the settings name.

use std::path::Path;

use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::config::RegistryFormat;
use crate::config::settings::Settings;

use super::error::FederationError;

/// Reads and checks the registry document, or the registry from the binding
/// that is its source, `settings` name, blocking the caller, or returns
/// `None` for neither.
///
/// The read fails with [`FederationError::Registry`] or
/// [`FederationError::FhirRegistry`] for a document that cannot be read or
/// refuses to load, and with the binding's error, such as
/// [`FederationError::Directory`] for a care services directory, for a source
/// that cannot be read or holds no valid registry; [`Federation::load_read`](super::Federation::load_read)
/// stops on that error.
#[must_use]
pub fn read_registry(settings: &Settings) -> Option<Result<RegistrySnapshot, FederationError>> {
    if let Some(read) = crate::binding::read_registry(settings) {
        return Some(read);
    }
    let path = settings.registry_document.as_deref()?;
    Some(read_document(path, settings.registry_format))
}

/// Reads the registry document at `path`, written in `format`.
fn read_document(path: &Path, format: RegistryFormat) -> Result<RegistrySnapshot, FederationError> {
    match format {
        RegistryFormat::Toml => {
            RegistrySnapshot::read(path).map_err(|source| FederationError::Registry {
                path: path.to_path_buf(),
                source: Box::new(source),
            })
        }
        #[cfg(feature = "binding-ihe")]
        RegistryFormat::Fhir => ferrofed_identity::ihe::mcsd::read(path).map_err(|source| {
            FederationError::FhirRegistry {
                path: path.to_path_buf(),
                source: Box::new(source),
            }
        }),
    }
}
