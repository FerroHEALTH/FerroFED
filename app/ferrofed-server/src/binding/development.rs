// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The development binding, `[dev]`, compiled into every build: the static
//! cross-reference, which also serves as the localizer, and the static
//! consent pre-filter, under `profile = "development"` alone.
//!
//! Both are FerroFED's own testing devices and bind nothing (no specification
//! governs them: our own design). The cross-reference is the localizer of
//! last resort: a binding's own localizer, or a resolver of a binding that
//! localizes, comes before it.

use std::fmt;
use std::sync::Arc;

use ferrofed_identity::consent::ConsentPrefilter;
use ferrofed_identity::dev::{DevTable, StaticConsentPrefilter, StaticResolver};
use ferrofed_identity::localizer::Localizer;
use ferrofed_identity::resolver::Resolver;
use ferrofed_registry::snapshot::RegistrySnapshot;
use serde::Deserialize;

use crate::binding::{Binding, Offer, Reload, ResolverSeam, Role, Section};
use crate::config::Config;
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::federation::error::FederationError;

/// The `localization.mode` of the static development cross-reference.
pub const DEVELOPMENT_STATIC: &str = "development-static";

/// The development binding.
#[derive(Debug, Clone, Copy)]
pub struct Development;

/// The sections the development binding reads.
const SECTIONS: &[Section] = &[Section {
    key: "dev",
    reload: Reload::Applies,
}];

/// The `[dev]` table, held as written until the registry it refers to is
/// loaded.
///
/// Its rows carry patient identifier values, so `Debug` shows how many rows
/// there are and none of them.
#[derive(Clone, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct DevSection(toml::Table);

impl DevSection {
    /// Reads the table as the static cross-reference's configuration.
    ///
    /// # Errors
    /// Returns [`Error::DevTable`] when the table does not have the shape of
    /// `[[dev.crossref]]` rows. The error names the shape, never a value.
    pub fn table(&self) -> Result<DevTable, Error> {
        toml::Value::Table(self.0.clone())
            .try_into::<DevTable>()
            .map_err(|_shape| Error::DevTable)
    }

    /// Whether the table has `[[dev.consent_denied]]` rows, which the static
    /// consent pre-filter is built from.
    #[must_use]
    pub fn denies_consent(&self) -> bool {
        self.0
            .get("consent_denied")
            .and_then(toml::Value::as_array)
            .is_some_and(|rows| !rows.is_empty())
    }
}

impl fmt::Debug for DevSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows = self
            .0
            .get("crossref")
            .and_then(toml::Value::as_array)
            .map_or(0, Vec::len);
        f.debug_struct("DevSection")
            .field("crossref_rows", &rows)
            .finish()
    }
}

impl Binding for Development {
    fn name(&self) -> &'static str {
        "development"
    }

    fn sections(&self) -> &'static [Section] {
        SECTIONS
    }

    fn resolve(&self, config: &Config, settings: &mut Settings) -> Result<(), Error> {
        settings.dev.clone_from(&config.dev);
        Ok(())
    }

    fn offers(&self, settings: &Settings) -> Vec<Offer> {
        let Some(section) = &settings.dev else {
            return Vec::new();
        };
        let mut offers = vec![Offer {
            role: Role::Resolver,
            section: "[dev]",
        }];
        if section.denies_consent() {
            offers.push(Offer {
                role: Role::ConsentPrefilter,
                section: "[[dev.consent_denied]]",
            });
        }
        offers
    }

    fn localizers(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (
            &[],
            &["the [dev] cross-reference under profile = \"development\""],
        )
    }

    fn resolvers(&self) -> &'static [&'static str] {
        &["[dev]"]
    }

    fn unregistered(&self, settings: &Settings) -> Result<(), FederationError> {
        if settings.dev.is_some() {
            return Err(FederationError::DevWithoutRegistry);
        }
        Ok(())
    }

    fn resolver(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<ResolverSeam>, FederationError> {
        let Some(section) = &settings.dev else {
            return Ok(None);
        };
        let table = section.table().map_err(FederationError::DevTable)?;
        let resolver = StaticResolver::from_config(settings.profile, Some(table), snapshot)
            .map_err(FederationError::DevCrossRef)?
            .map(Arc::new);
        Ok(resolver.map(|resolver| {
            let localizer: Arc<dyn Localizer> = resolver.clone();
            let resolver: Arc<dyn Resolver> = resolver;
            ResolverSeam {
                resolver,
                localizer: Some((localizer, DEVELOPMENT_STATIC)),
            }
        }))
    }

    fn prefilter(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
        let Some(section) = &settings.dev else {
            return Ok(None);
        };
        let table = section.table().map_err(FederationError::DevTable)?;
        Ok(
            StaticConsentPrefilter::from_config(settings.profile, &table, snapshot)
                .map_err(FederationError::DevCrossRef)?
                .map(|prefilter| -> Arc<dyn ConsentPrefilter> { Arc::new(prefilter) }),
        )
    }

    fn class(&self, error: &FederationError) -> Option<&'static str> {
        matches!(
            error,
            FederationError::DevWithoutRegistry
                | FederationError::DevTable(_)
                | FederationError::DevCrossRef(_)
        )
        .then_some("dev-cross-reference")
    }
}
