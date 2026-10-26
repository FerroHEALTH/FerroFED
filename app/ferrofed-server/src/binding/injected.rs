// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The injected roles, compiled only behind the `test-seam` feature, which
//! the test kit enables and no shipped build does.
//!
//! FerroFED keeps no patient index: a deployment resolves patients through a
//! PIX Manager. A test that needs a fixed cross-reference injects one here,
//! built by `ferrofed-testkit` over the registry snapshot, and the
//! [`Injected`] binding offers it as the federation's resolver, localizer
//! and consent pre-filter. A production build compiles none of this, so it
//! has no injection point (no specification governs this: our own design).

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_identity::role::consent::ConsentPrefilter;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::seam::ResolverSeam;
use crate::binding::{Binding, Offer, Role, Section};
use crate::config::Config;
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::federation::error::FederationError;

/// Builds the injected resolver over a registry snapshot, or names why the
/// snapshot refuses it.
pub type ResolverFactory =
    Arc<dyn Fn(&RegistrySnapshot) -> Result<ResolverSeam, String> + Send + Sync>;

/// Builds the injected consent pre-filter over a registry snapshot, or names
/// why the snapshot refuses it.
pub type PrefilterFactory =
    Arc<dyn Fn(&RegistrySnapshot) -> Result<Arc<dyn ConsentPrefilter>, String> + Send + Sync>;

/// The roles a test injects.
#[derive(Clone)]
pub struct Injection {
    /// The resolver, which also localizes.
    pub resolver: ResolverFactory,
    /// The consent pre-filter, when the test injects one.
    pub prefilter: Option<PrefilterFactory>,
}

impl fmt::Debug for Injection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Injection")
            .field("prefilter", &self.prefilter.is_some())
            .finish_non_exhaustive()
    }
}

/// The injection every federation this process builds takes; a test process
/// runs one test, so the injection is the test's.
static INJECTED: Mutex<Option<Injection>> = Mutex::new(None);

/// Injects `injection`, or clears the injection with `None`.
pub fn inject(injection: Option<Injection>) {
    *INJECTED.lock().unwrap_or_else(PoisonError::into_inner) = injection;
}

/// The current injection.
fn injected() -> Option<Injection> {
    INJECTED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The binding that offers the injected roles.
#[derive(Debug, Clone, Copy)]
pub struct Injected;

impl Binding for Injected {
    fn name(&self) -> &'static str {
        "injected"
    }

    fn sections(&self) -> &'static [Section] {
        &[]
    }

    fn resolve(&self, _config: &Config, _settings: &mut Settings) -> Result<(), Error> {
        Ok(())
    }

    fn offers(&self, _settings: &Settings) -> Vec<Offer> {
        let Some(injection) = injected() else {
            return Vec::new();
        };
        let mut offers = vec![Offer {
            role: Role::Resolver,
            section: "the injected cross-reference",
        }];
        if injection.prefilter.is_some() {
            offers.push(Offer {
                role: Role::ConsentPrefilter,
                section: "the injected consent pre-filter",
            });
        }
        offers
    }

    // NOTE: no specification governs this: our own design; a refusal names the sections an
    // operator can write, and an operator cannot write an injection.
    fn localizers(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&[], &[])
    }

    fn resolvers(&self) -> &'static [&'static str] {
        &[]
    }

    fn resolver(
        &self,
        _settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<ResolverSeam>, FederationError> {
        injected()
            .map(|injection| (injection.resolver)(snapshot).map_err(FederationError::Injected))
            .transpose()
    }

    fn prefilter(
        &self,
        _settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
        injected()
            .and_then(|injection| injection.prefilter)
            .map(|prefilter| prefilter(snapshot).map_err(FederationError::Injected))
            .transpose()
    }

    fn class(&self, error: &FederationError) -> Option<&'static str> {
        matches!(error, FederationError::Injected(_)).then_some("injected-cross-reference")
    }
}
