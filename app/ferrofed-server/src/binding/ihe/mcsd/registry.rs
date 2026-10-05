// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The registry read from an mCSD care services directory and kept in step
//! with it (`[registry.mcsd]`; §15.1, N21, Annex A.5).
//!
//! At boot the members are read with ITI-90 and checked as the registry
//! document in FHIR form is, and a directory that cannot be read, or holds
//! no valid registry, stops the boot. Every `refresh_interval_s` the changes
//! since the last read are asked for with ITI-91 off the clinical path, and
//! a changed registry goes through the same checks a reload does
//! ([`Reloader::directory_changed`]): the connection-type rule of §15.2
//! (N19, CP-20), one managing organisation per endpoint (N20), unique node,
//! endpoint and `system_id`s, the resolver's members and every other boot
//! check. A refresh that breaks one is refused: the running registry stays,
//! the refusal is logged and counted as a refused reload, and the next
//! refresh asks again from the same instant. Each read and refresh ends at
//! its deadline and its caps on pages, bytes and entries; one that runs past
//! them, or whose answer breaks ITI-91, is refused and counted the same way.
//! A directory that cannot be reached leaves the running registry in place;
//! every outcome shows on `/health/dependencies` as `directory`, and a
//! change the gateway refused shows the directory `degraded`, with the class
//! of the refusal as `directory_fault`, until a later refresh is accepted. A
//! query never waits on the directory. No specification governs the refresh
//! policy: our own design.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ferrofed_identity::directory::mcsd::{
    Content, DirectoryConfig, DirectoryConfigError, DirectoryReadError, DirectorySource,
    ExchangeError, Materialised, Refreshed,
};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use serde::Serialize;

use crate::binding::ihe::mcsd::DirectorySettings;
use crate::config::settings::Settings;
use crate::federation::{error::FederationError, registry::read_registry};
use crate::reload::{Applied, ReloadError, Reloader};
use crate::service;
use ferrofed_registry::health::Observed;

/// The directory a running gateway keeps its registry in step with.
#[derive(Debug)]
pub struct DirectoryRegistry {
    source: DirectorySource,
    held: Mutex<Content>,
    seen: Mutex<Seen>,
    interval: Duration,
}

/// The last state observed of the directory, and why it is degraded.
#[derive(Debug, Clone, Copy)]
struct Seen {
    state: Observed,
    fault: Option<DirectoryFault>,
}

/// Why the directory's last answer was not accepted, as `directory_fault`
/// names it.
///
/// The directory answered, and the gateway serves the registry it last
/// accepted: the directory is `degraded` when the gateway refused the
/// registry its change makes, and `failing` when it refused the gateway's
/// credentials. The fault names a class and never an identifier or the
/// directory's content; the refusal's log line names the precise class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum DirectoryFault {
    /// The changed registry breaks a rule of its own: the connection type of
    /// §15.2 (N19), one managing organisation per endpoint (N20), unique
    /// ids (N21), a registry left with no node, or the FHIR form.
    RegistryInvalid,
    /// The changed registry is sound, and the rest of the configuration does
    /// not fit its members: the resolver, the localizer, the credentials or
    /// another part of the federation refuses them.
    ConfigurationMismatch,
    /// The directory answered `401` or `403`: it refused the credentials of
    /// `[registry.mcsd]`, or what they grant.
    RefusedCredentials,
}

impl DirectoryFault {
    /// The fault as `directory_fault` names it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegistryInvalid => "registry-invalid",
            Self::ConfigurationMismatch => "configuration-mismatch",
            Self::RefusedCredentials => "refused-credentials",
        }
    }
}

/// What one refresh of the directory did.
#[derive(Debug)]
#[non_exhaustive]
pub enum RefreshOutcome {
    /// No member changed.
    Unchanged,
    /// The changed registry replaced the running one.
    Applied(Applied),
    /// The refresh was refused, and the running registry stays: the changed
    /// registry breaks a rule, or the directory's answer ran past the deadline
    /// or a cap, or does not hold to ITI-91.
    Refused(ReloadError),
    /// The directory could not be reached or refused the request, and the
    /// running registry stays.
    Unreachable(ExchangeError),
}

impl DirectoryRegistry {
    /// Reads the members from the directory `settings` name with ITI-90, and
    /// returns the directory with the registry it holds.
    ///
    /// The read runs on a runtime of its own, so it blocks the caller.
    ///
    /// # Errors
    /// [`FederationError::Directory`] for a directory that cannot be asked as
    /// configured, a read that cannot run, and a directory that
    /// cannot be read or holds no valid registry.
    pub fn open(settings: &DirectorySettings) -> Result<(Self, RegistrySnapshot), FederationError> {
        let source = source(settings)?;
        let materialised = blocking(&source)?;
        let (content, snapshot) = materialised.into_parts();
        let registry = Self {
            source,
            held: Mutex::new(content),
            seen: Mutex::new(Seen {
                state: Observed::Up,
                fault: None,
            }),
            interval: settings.refresh_interval,
        };
        Ok((registry, snapshot))
    }

    /// The last state observed of the directory, which
    /// `/health/dependencies` reports.
    #[must_use]
    pub fn observed(&self) -> Observed {
        self.seen().state
    }

    /// Why the directory's last answer was not accepted, which
    /// `/health/dependencies` reports as `directory_fault`; `None` unless the
    /// change it answered with was refused, or it refused the credentials.
    #[must_use]
    pub fn fault(&self) -> Option<DirectoryFault> {
        self.seen().fault
    }

    /// Asks the directory for the changes since the registry in place was
    /// read and, when there are some, hands the registry they make to
    /// `reloader`, which replaces the running one or refuses it.
    ///
    /// The directory content advances only with a registry that was applied,
    /// so a refused or failed refresh asks again from the same instant. The
    /// directory shows `up` only once its answer is accepted, and `degraded`
    /// while the change it answered with is refused.
    pub async fn refresh(&self, reloader: &Reloader) -> RefreshOutcome {
        let held = self.held().clone();
        let refreshed = match self.source.refresh(&held).await {
            Ok(refreshed) => refreshed,
            Err(error) => {
                self.record(seen(&error));
                tracing::warn!(
                    answered = error.answered(),
                    status = error.status().map(|status| status.as_u16()),
                    error = crate::chain(&error),
                    "the care services directory could not be read; the running registry stays"
                );
                // NOTE: no specification governs this: our own design; an answer past
                // the budget, or one that breaks ITI-91, is refused as a broken registry is.
                if error.exceeded() || (error.answered() && error.status().is_none()) {
                    return RefreshOutcome::Refused(reloader.directory_refused(directory_error(
                        DirectoryFailure::Read(DirectoryReadError::Exchange(error)),
                    )));
                }
                return RefreshOutcome::Unreachable(error);
            }
        };
        match refreshed {
            Refreshed::Unchanged(replica) => {
                *self.held() = replica;
                self.observe(Observed::Up);
                RefreshOutcome::Unchanged
            }
            Refreshed::Changed(materialised) => {
                let (replica, snapshot) = materialised.into_parts();
                match reloader.directory_changed(snapshot) {
                    Ok(applied) => {
                        *self.held() = replica;
                        self.observe(Observed::Up);
                        RefreshOutcome::Applied(applied)
                    }
                    Err(error) => {
                        self.refuse(fault_of(&error));
                        RefreshOutcome::Refused(error)
                    }
                }
            }
            Refreshed::Refused(error) => {
                self.refuse(DirectoryFault::RegistryInvalid);
                RefreshOutcome::Refused(reloader.directory_refused(directory_error(
                    DirectoryFailure::Read(DirectoryReadError::Registry(error)),
                )))
            }
        }
    }

    /// Refreshes the registry every interval until the process ends.
    pub async fn keep_in_step(self: Arc<Self>, reloader: Arc<Reloader>) {
        let start = tokio::time::Instant::now() + self.interval;
        let mut ticks = tokio::time::interval_at(start, self.interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            let outcome = self.refresh(&reloader).await;
            tracing::debug!(outcome = ?outcome, "care services directory refreshed");
        }
    }

    fn held(&self) -> MutexGuard<'_, Content> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn seen(&self) -> Seen {
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records `state` as what the directory's last exchange showed, which
    /// clears any fault.
    fn observe(&self, state: Observed) {
        self.record(Seen { state, fault: None });
    }

    fn record(&self, seen: Seen) {
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = seen;
    }

    /// Records that the directory answered with a change the gateway refused.
    // NOTE: no specification governs this: our own design; a refused change is
    // degraded, apart from no answer (`down`) and a broken one (`failing`).
    fn refuse(&self, fault: DirectoryFault) {
        self.record(Seen {
            state: Observed::Degraded,
            fault: Some(fault),
        });
    }
}

/// The fault a refused change shows: a registry that breaks a rule of its
/// own, or a sound one the rest of the configuration does not fit.
fn fault_of(error: &ReloadError) -> DirectoryFault {
    match error {
        ReloadError::Federation { source, .. }
            if matches!(
                **source,
                FederationError::Registry { .. }
                    | FederationError::FhirRegistry { .. }
                    | FederationError::Directory(_)
            ) =>
        {
            DirectoryFault::RegistryInvalid
        }
        ReloadError::Federation { .. }
        | ReloadError::Config(_)
        | ReloadError::RegistryPresence
        | ReloadError::Profile
        | ReloadError::Cleartext(_) => DirectoryFault::ConfigurationMismatch,
    }
}

/// Why the registry could not be read from the care services directory.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DirectoryFailure {
    /// The directory could not be read, or its content is no registry the
    /// gateway admits.
    #[error(transparent)]
    Read(DirectoryReadError),
    /// The directory cannot be asked as `[registry.mcsd]` describes it.
    #[error("the care services directory of [registry.mcsd] cannot be asked")]
    Source(#[source] DirectoryConfigError),
    /// The runtime a read outside the server runs on could not be built.
    #[error("the runtime for reading the care services directory could not be built")]
    Runtime(#[source] std::io::Error),
}

/// `failure` as the federation error a boot or a reload reports.
fn directory_error(failure: DirectoryFailure) -> FederationError {
    FederationError::Directory(Box::new(failure))
}

/// The registry's first read and, when it comes from a care services
/// directory, the directory the gateway then keeps it in step with.
///
/// A document is read as [`read_registry`] reads it; a directory is read
/// once here, so the banner, the build and the refreshes share one read.
#[must_use]
pub fn read_source(
    settings: &Settings,
) -> (
    Option<Result<RegistrySnapshot, FederationError>>,
    Option<Arc<DirectoryRegistry>>,
) {
    match &settings.registry_directory {
        None => (read_registry(settings), None),
        Some(directory) => match DirectoryRegistry::open(directory) {
            Ok((registry, snapshot)) => (Some(Ok(snapshot)), Some(Arc::new(registry))),
            Err(error) => (Some(Err(error)), None),
        },
    }
}

/// Reads the registry from the directory `settings` name, blocking the
/// caller, for a command that only checks it.
///
/// # Errors
/// The [`FederationError`] [`DirectoryRegistry::open`] returns.
pub fn read(settings: &DirectorySettings) -> Result<RegistrySnapshot, FederationError> {
    DirectoryRegistry::open(settings).map(|(_, snapshot)| snapshot)
}

/// The source `settings` name.
fn source(settings: &DirectorySettings) -> Result<DirectorySource, FederationError> {
    let credentials =
        service::authentication("registry.mcsd.credentials", settings.credentials.as_ref())?;
    let tls = service::tls_of("registry.mcsd", &settings.tls).map_err(FederationError::Tls)?;
    let source = DirectorySource::new(DirectoryConfig {
        base: settings.url.clone(),
        credentials,
        tls,
        deadline: settings.deadline,
        pages: settings.max_pages,
        bytes: settings.max_bytes,
        entries: settings.max_entries,
    })
    .map_err(|source| directory_error(DirectoryFailure::Source(source)))?;
    // NOTE: mCSD §2:3.90.5.1 and §2:3.91.5.1: each search and history is audited,
    // and one whose record is refused fails like a directory that did not answer.
    Ok(
        match crate::binding::ihe::audit::recorder(&settings.audit)
            .map_err(FederationError::Audit)?
        {
            Some(recorder) => source.audited(recorder),
            None => source,
        },
    )
}

/// Reads `source` on a current-thread runtime of a thread of its own, so the
/// read works whether or not the caller runs on a runtime.
fn blocking(source: &DirectorySource) -> Result<Materialised, FederationError> {
    std::thread::scope(|scope| {
        let reading = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|source| directory_error(DirectoryFailure::Runtime(source)))?;
            runtime
                .block_on(source.read())
                .map_err(|error| directory_error(DirectoryFailure::Read(error)))
        });
        match reading.join() {
            Ok(read) => read,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

/// What a failed exchange says of the directory: no answer is down, and any
/// answer the refresh could not use is failing, with a `401` or a `403`
/// named as refused credentials.
// NOTE: no specification governs this: our own design; a refresh answered with
// an error leaves the registry behind the directory, so it is never up.
fn seen(error: &ExchangeError) -> Seen {
    let status = error.status();
    let state = if status.is_some() || error.answered() {
        Observed::Failing
    } else {
        Observed::Down
    };
    let fault = status
        .filter(|status| *status == StatusCode::UNAUTHORIZED || *status == StatusCode::FORBIDDEN)
        .map(|_| DirectoryFault::RefusedCredentials);
    Seen { state, fault }
}
