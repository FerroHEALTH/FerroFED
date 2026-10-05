// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the IHE binding runs for the life of the process.
//!
//! Beside every federation a reload builds run the mCSD care services
//! directory the registry is kept in step with (Annex A.5) and the PMIR
//! identity feed (Annex A.4). No specification governs the process model:
//! our own design.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::routing::post;

use crate::binding::ihe::mcsd::registry::DirectoryRegistry;
use crate::binding::ihe::pmir::IdentityFeed;
use crate::binding::ihe::pmir::subscription;
use crate::config::settings::Settings;
use crate::reload::Reloader;
use crate::state::{AppState, StateError};
use ferrofed_registry::health::Indication;

/// The directory and the identity feed of a running gateway.
#[derive(Debug, Default)]
pub struct Processes {
    /// The care services directory the registry is kept in step with, when
    /// it is read from one; it outlives every federation a refresh builds.
    pub(crate) directory: Option<Arc<DirectoryRegistry>>,
    /// The PMIR identity feed, when `[pmir]` is set; it outlives every
    /// federation a reload builds, whose bindings it drives.
    pub(crate) identity_feed: Option<Arc<IdentityFeed>>,
}

impl Processes {
    /// Returns what `settings` describe: the identity feed, scoped by the
    /// `ehr_id` domain of every member `[pixm]` maps (Annex A.1). The
    /// directory is opened before the state, by the registry's first read.
    ///
    /// # Errors
    ///
    /// [`StateError::IdentityFeed`] and [`StateError::Audit`] for a feed that
    /// cannot be built.
    pub(crate) fn build(settings: &Settings) -> Result<Self, StateError> {
        Ok(Self {
            directory: None,
            identity_feed: identity_feed(settings)?,
        })
    }

    /// Returns the state of the directory and of the identity feed's
    /// Registry, each with why it is not up.
    pub(crate) fn indicate(&self) -> Vec<(&'static str, Indication)> {
        let mut indications = Vec::new();
        if let Some(directory) = &self.directory {
            indications.push(("directory", Indication::State(directory.observed())));
            if let Some(fault) = directory.fault() {
                indications.push((
                    "directory_fault",
                    Indication::Fault(fault.as_str().to_owned()),
                ));
            }
        }
        if let Some(feed) = &self.identity_feed {
            indications.push(("identity_registry", Indication::State(feed.observed())));
            if let Some(fault) = feed.fault() {
                indications.push((
                    "identity_registry_fault",
                    Indication::Fault(fault.as_str().to_owned()),
                ));
            }
        }
        indications
    }

    /// Returns `surface` with the identity feed's route, when `[pmir]` is set.
    pub(crate) fn routes(&self, surface: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
        // NOTE: PMIR §2:3.93.5: the feed authenticates its Supplier with its own token,
        // so its route sits outside the ITS-REST surface and its client authentication.
        match &self.identity_feed {
            Some(feed) => surface.route(feed.path(), post(super::pmir::route::feed)),
            None => surface,
        }
    }

    /// Starts keeping the registry in step with the directory, through
    /// `reloader`, and the identity feed's subscription.
    pub(crate) fn start(&self, reloader: &Arc<Reloader>) -> Running {
        if let Some(directory) = &self.directory {
            tokio::spawn(Arc::clone(directory).keep_in_step(Arc::clone(reloader)));
        }
        Running(self.identity_feed.as_ref().map(IdentityFeed::start))
    }
}

/// The identity feed's subscription loop, when one runs.
#[derive(Debug)]
pub struct Running(Option<subscription::Running>);

impl Running {
    /// Stops the subscription loop and deletes the subscription.
    pub(crate) async fn drain(self) {
        if let Some(subscription) = self.0 {
            subscription.drain().await;
        }
    }
}

/// The PMIR identity feed `settings` describe, scoped by the `ehr_id`
/// domain of every member `[pixm]` maps (Annex A.1).
fn identity_feed(settings: &Settings) -> Result<Option<Arc<IdentityFeed>>, StateError> {
    let Some(pmir) = &settings.pmir else {
        return Ok(None);
    };
    let domains: BTreeSet<String> = settings
        .pixm
        .iter()
        .flat_map(|pixm| pixm.managers.iter())
        .flat_map(|manager| manager.members.values().cloned())
        .collect();
    let audit = super::audit::recorder(&settings.audit).map_err(StateError::Audit)?;
    Ok(Some(Arc::new(IdentityFeed::new(pmir, domains, audit)?)))
}

impl AppState {
    /// Returns this state keeping its registry in step with `directory`,
    /// whose last observed state `/health/dependencies` reports.
    #[must_use]
    pub fn watching(mut self, directory: Arc<DirectoryRegistry>) -> Self {
        self.processes_mut().ihe.directory = Some(directory);
        self
    }

    /// Returns this state applying the ITI-93 messages of `feed` to its
    /// federation's resolution bindings.
    #[must_use]
    pub fn with_identity_feed(mut self, feed: Arc<IdentityFeed>) -> Self {
        self.processes_mut().ihe.identity_feed = Some(feed);
        self
    }

    /// Returns the PMIR identity feed, when `[pmir]` is set.
    #[must_use]
    pub fn identity_feed(&self) -> Option<&Arc<IdentityFeed>> {
        self.processes().ihe.identity_feed.as_ref()
    }

    /// Returns the care services directory the registry is kept in step
    /// with, when it is read from one.
    #[must_use]
    pub fn directory(&self) -> Option<&Arc<DirectoryRegistry>> {
        self.processes().ihe.directory.as_ref()
    }
}
