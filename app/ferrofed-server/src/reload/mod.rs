// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reloading the registry while the gateway serves.
//!
//! On `SIGHUP` the server reads its configuration again from the source it
//! started from, the file `--config` or `FERROFED_CONFIG` names and the
//! `FERROFED__` environment, and checks it exactly as at boot:
//! [`Config::load`], [`Config::resolve`], then
//! [`Federation::reloaded`](crate::federation::Federation::reloaded),
//! which is [`Federation::load`](crate::federation::Federation::load) over
//! what the process has learned, and [`transport::check`] under the profile
//! the process started with. A valid
//! registry replaces the running one at once; a request that already took
//! the running one finishes on it. Learned `creating_system_id` routes the
//! new document contradicts are withdrawn with their incidents, and index
//! entries and resolution bindings naming a member that left are dropped.
//! A configuration that does not load leaves the running registry in place.
//!
//! The same signal reads the certificate, the key and the client CA of each
//! listener that serves TLS again from its files
//! ([`Reloader::reload_certificates`]); a changed path takes a restart.
//!
//! The sections [`reloadable`](crate::binding::reloadable) names take effect
//! on a reload: the registry, the onward credentials, and each section a
//! binding declares as one a reload applies
//! ([`Binding::sections`](crate::binding::Binding::sections)). A changed
//! `profile` refuses the reload, so every decision the development profile
//! admits reads the profile the process started with. Every other setting is
//! compared with the value the process started with, and a change is logged
//! as needing a restart while the rest of the reload applies.
//!
//! Under `config.ui = "managed"` the file read is first recorded as a
//! configuration revision when it changed, and the reload applies the
//! revision the store marks running ([`Revisions::file_loaded`]); an
//! operator's apply of a revision takes the same path
//! ([`Reloader::reload_config`]). No specification governs this: our own
//! design.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_engine::dispatch::SetupError;
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::{self, Role, RoleConflict};
use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, ProtectedSite};
use crate::config::{CONFIG_PATH_ENV, Config};
use crate::federation::{Federation, Reconciled, error::FederationError, registry::read_registry};
use crate::listener::certificates::Certificates;
use crate::metrics::ReloadResult;
use crate::revision::RevisionId;
use crate::revision::service::{RevisionError, Revisions};
use crate::state::AppState;

mod restart;

use restart::{effective, needs_restart};

/// Reloads the registry the server started with.
///
/// It holds the settings the process started with, so a reload applies only
/// the sections [`reloadable`](crate::binding::reloadable) names and reports
/// a change to any other one. A registry a binding reads from its own
/// source, such as a care services directory, changes with that source,
/// never with a reload: a reload rebuilds the federation over the registry in
/// place.
pub struct Reloader {
    config: Option<PathBuf>,
    boot: Settings,
    state: Arc<AppState>,
    serial: Mutex<Option<Settings>>,
    certificates: Vec<Arc<Certificates>>,
    revisions: Option<Arc<Revisions>>,
}

/// What an applied reload changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Applied {
    /// How many members the registry now holds.
    pub members: usize,
    /// The endpoints the new registry adds, in id order.
    pub endpoints_added: Vec<EndpointId>,
    /// The endpoints the new registry no longer holds, in id order: no
    /// request that starts after the reload calls them.
    pub endpoints_removed: Vec<EndpointId>,
    /// The members the new registry no longer holds, in id order.
    pub members_removed: Vec<NodeId>,
    /// What the reload did to what the process had learned.
    pub reconciled: Reconciled,
    /// The changed settings that take effect only on a restart, by key.
    pub needs_restart: Vec<&'static str>,
    /// The credentials that travel unencrypted, which only the
    /// development profile allows ([`transport::check`]).
    pub cleartext: Vec<ProtectedSite>,
}

/// A reload that was refused, leaving the running registry in place.
///
/// Its message names the failure and never a value of the configuration or
/// the document.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    /// The configuration could not be read or resolved.
    #[error("the configuration could not be loaded")]
    Config(#[source] crate::config::error::Error),
    /// The registry the configuration describes could not be built.
    #[error("the registry could not be loaded")]
    Federation {
        /// The registry document the configuration names.
        document: Option<PathBuf>,
        /// What the federation reported.
        #[source]
        source: Box<FederationError>,
    },
    /// The registry's source, `registry.document` or `[registry.mcsd]`, was
    /// set, unset or changed since the process started.
    #[error(
        "the registry source (registry.document or [registry.mcsd]) was set, unset or changed, which takes a restart"
    )]
    RegistryPresence,
    /// `profile` differs from the profile the process started with. Every
    /// decision the development profile admits reads the boot profile, so a
    /// change takes a restart.
    #[error("profile was changed, which takes a restart")]
    Profile,
    /// A credential or a patient identifier would travel over a URL that is
    /// not `https`, outside the development profile the process started with.
    #[error("a credential or a patient identifier would travel in cleartext")]
    Cleartext(#[source] CleartextError),
    /// Under `config.ui = "managed"`, the file's configuration could not be
    /// recorded as a revision, or the running revision is refused beside it.
    #[error("the configuration revisions refused the reload")]
    Revisions(#[source] Box<RevisionError>),
}

impl ReloadError {
    /// The class of the failure, the field the refusal is logged under.
    #[must_use]
    pub fn class(&self) -> &'static str {
        match self {
            Self::Config(_) => "configuration",
            Self::Federation { source, .. } => federation_class(source),
            Self::RegistryPresence => "registry-presence",
            Self::Profile => "profile",
            Self::Cleartext(_) => "cleartext",
            Self::Revisions(_) => "revisions",
        }
    }

    /// The registry document the refusal is about, when it is about one.
    #[must_use]
    pub fn document(&self) -> Option<&std::path::Path> {
        match self {
            Self::Federation { document, .. } => document.as_deref(),
            Self::Config(_)
            | Self::RegistryPresence
            | Self::Profile
            | Self::Cleartext(_)
            | Self::Revisions(_) => None,
        }
    }
}

impl Reloader {
    /// A reloader that reads the configuration from `config`, or from what
    /// `FERROFED_CONFIG` names when `config` is `None`, as the process did at
    /// boot with `boot` as the outcome, and swaps the federation of `state`.
    #[must_use]
    pub fn new(config: Option<PathBuf>, boot: Settings, state: Arc<AppState>) -> Self {
        Self {
            config,
            boot,
            state,
            serial: Mutex::new(None),
            certificates: Vec::new(),
            revisions: None,
        }
    }

    /// Returns this reloader with `revisions`, so a reload under
    /// `config.ui = "managed"` records the file's configuration as a
    /// revision and runs the revision the store marks current
    /// ([`Revisions::file_loaded`]).
    #[must_use]
    pub fn with_revisions(mut self, revisions: Option<Arc<Revisions>>) -> Self {
        self.revisions = revisions;
        self
    }

    /// Applies `config`, configuration revision `revision` with the file's
    /// bootstrap core beside it, as a reload applies the file: the same
    /// checks, the same sections applied and the same changes left for a
    /// restart; records that the gateway runs `revision` when it applied;
    /// logs the outcome and returns it.
    ///
    /// # Errors
    /// Every [`ReloadError`] a reload of the file returns; the running
    /// registry then stays in place.
    pub fn reload_config(
        &self,
        config: &Config,
        revision: RevisionId,
    ) -> Result<Applied, ReloadError> {
        let mut applied = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let outcome = config
            .resolve()
            .map_err(ReloadError::Config)
            .and_then(|fresh| self.apply_settings(fresh))
            .map(|(outcome, effective)| {
                *applied = Some(effective);
                self.ran(revision);
                outcome
            });
        drop(applied);
        self.log(&outcome);
        outcome
    }

    /// Returns the state whose federation a reload replaces.
    #[must_use]
    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }

    /// Whether the registry is read from a binding's source, such as a care
    /// services directory, rather than a document; a restart alone changes
    /// it.
    #[must_use]
    pub fn reads_directory(&self) -> bool {
        binding::sources_registry(&self.boot)
    }

    /// Returns this reloader with `certificates`, the TLS of the listeners,
    /// which [`Reloader::reload_certificates`] reads again from their files.
    #[must_use]
    pub fn with_certificates(mut self, certificates: Vec<Arc<Certificates>>) -> Self {
        self.certificates = certificates;
        self
    }

    /// Reads the certificate, the key and the client CA of every listener
    /// that serves TLS again from the files it started with, logs each
    /// outcome, and returns the tables whose files were refused.
    ///
    /// A listener whose files read puts them in place for every handshake
    /// that starts afterwards; one whose files do not keeps the certificate it
    /// serves. No connection already open changes.
    pub fn reload_certificates(&self) -> Vec<&'static str> {
        let mut refused = Vec::new();
        for certificates in &self.certificates {
            let table = certificates.files().table;
            match certificates.reload() {
                Ok(()) => tracing::info!(table, "listener certificate reloaded"),
                Err(error) => {
                    tracing::error!(
                        table,
                        error = crate::chain(&error),
                        "listener certificate reload refused, the running certificate stays"
                    );
                    refused.push(table);
                }
            }
        }
        refused
    }

    /// Reloads the registry, logs the outcome, and returns it.
    ///
    /// The signal handler calls this; one reload runs at a time. A registry
    /// read from a care services directory stays the one in place, and the
    /// federation is rebuilt over it with the reloaded sections.
    ///
    /// # Errors
    /// Returns a [`ReloadError`] when the configuration or the registry does
    /// not load, or the registry's source was set, unset or changed between
    /// a document and a directory; the running registry then stays in place.
    pub fn reload(&self) -> Result<Applied, ReloadError> {
        // NOTE: no specification governs this: our own design; the configuration is read
        // before the reload lock, which an apply of a revision takes inside its own.
        let fresh = self.fresh();
        let mut applied = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let mut loaded = None;
        let outcome = fresh
            .and_then(|(fresh, revision)| {
                loaded = revision;
                self.apply_settings(fresh)
            })
            .map(|(outcome, effective)| {
                *applied = Some(effective);
                if let Some((revision, _recorded)) = loaded {
                    self.ran(revision);
                }
                outcome
            });
        drop(applied);
        self.log(&outcome);
        if let (Ok(applied), Some((revision, true)), Some(revisions)) =
            (&outcome, loaded, &self.revisions)
            && let Err(error) = revisions.file_applied(revision, &applied.needs_restart)
        {
            tracing::error!(
                revision = %revision,
                error = crate::chain(&error),
                "the file's revision could not be marked pending a restart"
            );
        }
        outcome
    }

    /// Records, under the reload lock, that the gateway runs `revision`.
    fn ran(&self, revision: RevisionId) {
        if let Some(revisions) = &self.revisions {
            revisions.ran(revision);
        }
    }

    /// Puts the registry a refresh of the care services directory read in
    /// place of the running one, checked as a reload is, with the settings
    /// the last reload applied; logs the outcome and counts it.
    ///
    /// # Errors
    /// Returns [`ReloadError::Federation`] when the federation cannot be built
    /// over `snapshot`; the running registry then stays in place.
    pub fn directory_changed(&self, snapshot: RegistrySnapshot) -> Result<Applied, ReloadError> {
        let applied = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let settings = applied.as_ref().unwrap_or(&self.boot);
        let outcome = match self.state.federation() {
            None => Err(ReloadError::RegistryPresence),
            Some(running) => self.swap(&running, settings, Some(Ok(snapshot)), Vec::new()),
        };
        drop(applied);
        self.log(&outcome);
        outcome
    }

    /// Logs and counts a refresh of the care services directory whose
    /// registry was refused before a federation could be built over it, and
    /// returns the refusal; the running registry stays in place.
    pub fn directory_refused(&self, refusal: FederationError) -> ReloadError {
        let error = ReloadError::Federation {
            document: None,
            source: Box::new(refusal),
        };
        self.log_refusal(&error);
        error
    }

    /// Reads the configuration again: the file's, or under
    /// `config.ui = "managed"` the revision the store marks current once
    /// the file's has been recorded, with the revision that is and whether
    /// the file's load recorded it.
    fn fresh(&self) -> Result<(Settings, Option<(RevisionId, bool)>), ReloadError> {
        let Some(revisions) = &self.revisions else {
            return Config::load(self.config.as_deref())
                .and_then(|config| config.resolve())
                .map(|settings| (settings, None))
                .map_err(ReloadError::Config);
        };
        let (config, tree) =
            Config::load_with_tree(self.config.as_deref()).map_err(ReloadError::Config)?;
        let settings = config.resolve().map_err(ReloadError::Config)?;
        crate::config::revisions::check_tree(settings.revisions.as_ref(), &tree)
            .map_err(ReloadError::Config)?;
        let loaded = revisions
            .file_loaded(&tree)
            .map_err(|error| ReloadError::Revisions(Box::new(error)))?;
        let settings = loaded.config.resolve().map_err(ReloadError::Config)?;
        Ok((settings, Some((loaded.revision, loaded.recorded))))
    }

    /// Swaps in the registry `fresh` describes, returning what changed and
    /// the settings now in effect.
    fn apply_settings(&self, fresh: Settings) -> Result<(Applied, Settings), ReloadError> {
        if source_kind(&fresh) != source_kind(&self.boot) {
            return Err(ReloadError::RegistryPresence);
        }
        // NOTE: no specification governs this: our own design; a reload under
        // another profile could admit what only development allows, so it is refused.
        if fresh.profile != self.boot.profile {
            return Err(ReloadError::Profile);
        }
        // NOTE: no specification governs this: our own design; a setting that
        // waits for a restart is held now, so the restart cannot stop on it.
        transport::check(&fresh, None).map_err(ReloadError::Cleartext)?;
        let needs_restart = needs_restart(&self.boot, &fresh);
        let effective = effective(&self.boot, fresh);
        let Some(running) = self.state.federation() else {
            return Ok((
                Applied {
                    needs_restart,
                    ..Applied::default()
                },
                effective,
            ));
        };
        // NOTE: no specification governs this: our own design; the source, not the
        // configuration, changes a binding source's registry, so a reload keeps it.
        let document = if binding::sources_registry(&effective) {
            Some(Ok(running.snapshot().clone()))
        } else {
            read_registry(&effective)
        };
        let applied = self.swap(&running, &effective, document, needs_restart)?;
        Ok((applied, effective))
    }

    /// Builds the federation `settings` describe over `document` in place of
    /// `running`, and swaps it in.
    fn swap(
        &self,
        running: &Arc<Federation>,
        settings: &Settings,
        document: Option<Result<RegistrySnapshot, FederationError>>,
        needs_restart: Vec<&'static str>,
    ) -> Result<Applied, ReloadError> {
        let next = running
            .reloaded(settings, document)
            .map_err(|source| ReloadError::Federation {
                document: settings.registry_document.clone(),
                source: Box::new(source),
            })?
            .ok_or(ReloadError::RegistryPresence)?;
        let cleartext =
            transport::check(settings, Some(next.snapshot())).map_err(ReloadError::Cleartext)?;
        let next = Arc::new(next);
        let (before, after) = (running.snapshot(), next.snapshot());
        let members_removed: Vec<NodeId> = before
            .nodes()
            .map(|node| node.id().clone())
            .filter(|node| after.node(node).is_none())
            .collect();
        let endpoints_removed = before
            .endpoints()
            .map(|endpoint| endpoint.id().clone())
            .filter(|endpoint| after.endpoint(endpoint).is_none())
            .collect();
        let endpoints_added = after
            .endpoints()
            .map(|endpoint| endpoint.id().clone())
            .filter(|endpoint| before.endpoint(endpoint).is_none())
            .collect();
        self.state.replace_federation(Arc::clone(&next));
        let departed: BTreeSet<NodeId> = members_removed.iter().cloned().collect();
        let reconciled = next.reconcile(&departed);
        Ok(Applied {
            members: after.nodes().count(),
            endpoints_added,
            endpoints_removed,
            members_removed,
            reconciled,
            needs_restart,
            cleartext,
        })
    }

    /// Logs an outcome, ids and counts, never a value of the configuration,
    /// the document, a credential or a header, and counts it on the metrics
    /// surface.
    fn log(&self, outcome: &Result<Applied, ReloadError>) {
        match outcome {
            Ok(applied) => {
                self.state.metrics().reloaded(ReloadResult::Applied);
                tracing::info!(
                    members = applied.members,
                    endpoints_added = joined(&applied.endpoints_added, EndpointId::as_str),
                    endpoints_removed = joined(&applied.endpoints_removed, EndpointId::as_str),
                    members_removed = joined(&applied.members_removed, NodeId::as_str),
                    incidents = applied.reconciled.incidents.len(),
                    index_dropped = applied.reconciled.index_dropped,
                    bindings_dropped = applied.reconciled.bindings_dropped,
                    "registry reloaded"
                );
                transport::warn(&applied.cleartext);
                if !applied.needs_restart.is_empty() {
                    tracing::warn!(
                        settings = applied.needs_restart.join(","),
                        "changed settings take effect only on a restart; the running values stay"
                    );
                }
            }
            Err(error) => self.log_refusal(error),
        }
    }

    /// Logs a refusal by its class, never a value, and counts it.
    fn log_refusal(&self, error: &ReloadError) {
        self.state.metrics().reloaded(ReloadResult::Refused);
        tracing::error!(
            class = error.class(),
            config = self.source().map(|path| path.display().to_string()),
            document = error.document().map(|path| path.display().to_string()),
            "registry reload refused, the running registry stays; `ferrofed config check` names the fault"
        );
    }

    /// The configuration file a reload reads, when one is named.
    fn source(&self) -> Option<PathBuf> {
        self.config
            .clone()
            .or_else(|| std::env::var_os(CONFIG_PATH_ENV).map(PathBuf::from))
    }
}

/// Where `settings` read the registry from: a document, a binding's source,
/// or nowhere.
fn source_kind(settings: &Settings) -> (bool, bool) {
    (
        settings.registry_document.is_some(),
        binding::sources_registry(settings),
    )
}

impl std::fmt::Debug for Reloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reloader")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Reloads the registry, and the certificate of every listener that serves
/// TLS, each time the process receives `SIGHUP`.
///
/// The reload reads files, so it runs on the blocking pool. A failure to
/// install the handler is logged, and the registry then changes only on a
/// restart.
pub async fn on_hangup(reloader: Arc<Reloader>) {
    use tokio::signal::unix::{SignalKind, signal};

    // NOTE: no specification governs this: our own design; SIGHUP is the
    // daemon reload signal, and a file watch could read a half-written document.
    let mut hangup = match signal(SignalKind::hangup()) {
        Ok(hangup) => hangup,
        Err(error) => {
            tracing::error!(%error, "cannot listen for SIGHUP; the registry reloads only on a restart");
            return;
        }
    };
    while hangup.recv().await.is_some() {
        tracing::info!("SIGHUP received, reloading the registry and the listener certificates");
        let reloader = Arc::clone(&reloader);
        let outcome = tokio::task::spawn_blocking(move || {
            let registry = reloader.reload();
            reloader.reload_certificates();
            registry
        });
        if let Err(error) = outcome.await {
            tracing::error!(
                panicked = error.is_panic(),
                "the registry reload did not finish; the running registry stays"
            );
        }
    }
}

/// The class a refused federation is logged under: the core's own, or the
/// class the binding whose error it is names.
fn federation_class(error: &FederationError) -> &'static str {
    match error {
        FederationError::Registry { source, .. } => match **source {
            LoadError::Read { .. } => "registry-unreadable",
            _ => "registry-invalid",
        },
        FederationError::Conflict(RoleConflict { role, .. }) => match role {
            Role::Resolver => "resolvers",
            Role::ConsentPrefilter => "consent-prefilter",
            Role::Localizer => "localization",
            _ => "bindings",
        },
        FederationError::Localization(_) => "localization",
        FederationError::NodeSelectionUndeclared | FederationError::IdUndeclared => "federation",
        FederationError::DemographicWithoutRegistry
        | FederationError::DemographicEndpointUnknown { .. } => "demographic-endpoint",
        FederationError::PatientWithoutRegistry { .. }
        | FederationError::PatientEndpointUnknown { .. }
        | FederationError::PatientWithoutResolver { .. } => "patient-binding",
        FederationError::Describe(_) => "self-description",
        FederationError::Clients(SetupError::UnknownEndpoint { .. })
        | FederationError::Grant { .. } => "credentials",
        FederationError::Tls(_) => "tls",
        FederationError::Clients(_) => "node-clients",
        FederationError::Transport(_) => "http-client",
        FederationError::Unsigned => "signing",
        FederationError::RetentionEndpointUnknown { .. } => "access-log",
        FederationError::Audit(_) => "audit",
        _ => binding::class(error).unwrap_or("binding"),
    }
}

/// The ids in `ids`, comma-separated.
fn joined<T>(ids: &[T], name: impl Fn(&T) -> &str) -> String {
    ids.iter().map(name).collect::<Vec<_>>().join(",")
}
