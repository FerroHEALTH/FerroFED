// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Step-1 seams the `[dev]` table builds under the development profile:
//! the static cross-reference, which also serves as the localizer, and the
//! static consent pre-filter. Both are FerroFED's own testing devices and bind
//! nothing (no specification governs them: our own design).

use std::sync::Arc;

use ferrofed_identity::consent::ConsentPrefilter;
use ferrofed_identity::dev::{StaticConsentPrefilter, StaticResolver};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::config::DevSection;
use crate::config::settings::Settings;
use crate::federation::error::FederationError;

/// The resolver and the consent pre-filter of the `[dev]` table.
pub(crate) type Seams = (
    Option<Arc<StaticResolver>>,
    Option<Arc<dyn ConsentPrefilter>>,
);

/// The static cross-reference and, when `[[dev.consent_denied]]` has rows,
/// the static consent pre-filter that the `[dev]` table describes.
///
/// # Errors
///
/// [`FederationError::DevTable`] when the table does not read as the
/// development table, and [`FederationError::DevCrossRef`] when a row or the
/// profile is refused.
pub(crate) fn seams(
    settings: &Settings,
    section: &DevSection,
    snapshot: &RegistrySnapshot,
) -> Result<Seams, FederationError> {
    let table = section.table().map_err(FederationError::DevTable)?;
    let consent = StaticConsentPrefilter::from_config(settings.profile, &table, snapshot)
        .map_err(FederationError::DevCrossRef)?
        .map(|prefilter| -> Arc<dyn ConsentPrefilter> { Arc::new(prefilter) });
    let resolver = StaticResolver::from_config(settings.profile, Some(table), snapshot)
        .map_err(FederationError::DevCrossRef)?
        .map(Arc::new);
    Ok((resolver, consent))
}
