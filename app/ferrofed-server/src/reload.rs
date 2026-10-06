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
//! as needing a restart while the rest of the reload applies. No
//! specification governs this: our own design.

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
use crate::state::AppState;

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
        }
    }

    /// The registry document the refusal is about, when it is about one.
    #[must_use]
    pub fn document(&self) -> Option<&std::path::Path> {
        match self {
            Self::Federation { document, .. } => document.as_deref(),
            Self::Config(_) | Self::RegistryPresence | Self::Profile | Self::Cleartext(_) => None,
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
        }
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
        let mut applied = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        let outcome = self.apply().map(|(outcome, effective)| {
            *applied = Some(effective);
            outcome
        });
        drop(applied);
        self.log(&outcome);
        outcome
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

    /// Reads the configuration again and swaps in the registry it describes,
    /// returning what changed and the settings now in effect.
    fn apply(&self) -> Result<(Applied, Settings), ReloadError> {
        let fresh = Config::load(self.config.as_deref())
            .and_then(|config| config.resolve())
            .map_err(ReloadError::Config)?;
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

/// The settings a reload builds the federation from: the sections of `fresh`
/// a reload applies, and every other setting as the process started, each
/// binding keeping the boot's value of what it does not reload.
fn effective(boot: &Settings, fresh: Settings) -> Settings {
    let mut effective = Settings {
        profile: boot.profile,
        server: boot.server.clone(),
        telemetry: boot.telemetry.clone(),
        federation: boot.federation.clone(),
        stored_queries: boot.stored_queries.clone(),
        fhir: boot.fhir.clone(),
        metrics: boot.metrics.clone(),
        signing: boot.signing.clone(),
        ..fresh
    };
    for binding in binding::compiled() {
        binding.effective(boot, &mut effective);
    }
    effective
}

/// Whether `fresh` names another stored-query store than the process
/// started with, or sets or unsets one.
fn stored_queries_changed(boot: &Settings, fresh: &Settings) -> bool {
    match (&boot.stored_queries, &fresh.stored_queries) {
        (Some(was), Some(now)) => !was.same_as(now),
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
    }
}

/// Whether `fresh` names other signing keys, another JWK Set location or
/// another assertion lifetime than the process started with; the keys are
/// compared by `kid`, never by their material.
fn signing_changed(boot: &Settings, fresh: &Settings) -> bool {
    let shape = |settings: &Settings| {
        settings.signing.as_ref().map(|signing| {
            (
                signing.keys.current().kid().to_owned(),
                signing.keys.retiring().map(|key| key.kid().to_owned()),
                signing.keys.next().map(|key| key.kid().to_owned()),
                signing.jwks_uri.as_str().to_owned(),
                signing.assertion_lifetime,
            )
        })
    };
    shape(boot) != shape(fresh)
}

/// A key a reload does not apply, with whether its value differs between the
/// settings the process started with and the fresh ones.
type RestartKey = (&'static str, fn(&Settings, &Settings) -> bool);

/// The core's keys a reload does not apply, in the order a refusal names
/// them.
const RESTART_KEYS: &[RestartKey] = &[
    ("signing", signing_changed),
    ("server.listen", |boot, fresh| {
        boot.server.listen != fresh.server.listen
    }),
    ("server.tls", |boot, fresh| {
        boot.server.tls != fresh.server.tls
    }),
    ("metrics.tls", |boot, fresh| {
        boot.metrics.tls != fresh.metrics.tls
    }),
    ("server.base_path", |boot, fresh| {
        boot.server.base_path != fresh.server.base_path
    }),
    ("server.request_timeout_ms", |boot, fresh| {
        boot.server.request_timeout != fresh.server.request_timeout
    }),
    ("server.drain_delay_ms", |boot, fresh| {
        boot.server.drain_delay != fresh.server.drain_delay
    }),
    ("server.shutdown_timeout_ms", |boot, fresh| {
        boot.server.shutdown_timeout != fresh.server.shutdown_timeout
    }),
    ("server.bindings_drain_timeout_ms", |boot, fresh| {
        boot.server.bindings_drain != fresh.server.bindings_drain
    }),
    ("server.body_limit_bytes", |boot, fresh| {
        boot.server.body_limit != fresh.server.body_limit
    }),
    ("server.overload", |boot, fresh| {
        boot.server.overload != fresh.server.overload
    }),
    ("telemetry.format", |boot, fresh| {
        boot.telemetry.format != fresh.telemetry.format
    }),
    ("telemetry.filter", |boot, fresh| {
        boot.telemetry.filter != fresh.telemetry.filter
    }),
    ("telemetry.otlp_endpoint", |boot, fresh| {
        boot.telemetry.otlp_endpoint != fresh.telemetry.otlp_endpoint
    }),
    ("telemetry.trace_sample_ratio", |boot, fresh| {
        boot.telemetry.trace_sample_ratio != fresh.telemetry.trace_sample_ratio
    }),
    ("federation.id", |boot, fresh| {
        boot.federation.id != fresh.federation.id
    }),
    ("federation.timeouts", |boot, fresh| {
        boot.federation.budget != fresh.federation.budget
    }),
    ("federation.default_namespace", |boot, fresh| {
        boot.federation.default_namespace != fresh.federation.default_namespace
    }),
    ("federation.binding_ttl_ms", |boot, fresh| {
        boot.federation.binding_ttl != fresh.federation.binding_ttl
    }),
    ("federation.binding_capacity", |boot, fresh| {
        boot.federation.binding_capacity != fresh.federation.binding_capacity
    }),
    ("federation.ehr_index_capacity", |boot, fresh| {
        boot.federation.ehr_index_capacity != fresh.federation.ehr_index_capacity
    }),
    ("federation.node_selection", |boot, fresh| {
        boot.federation.node_selection != fresh.federation.node_selection
    }),
    ("federation.best_effort", |boot, fresh| {
        boot.federation.best_effort != fresh.federation.best_effort
    }),
    ("federation.max_in_flight_per_node", |boot, fresh| {
        boot.federation.max_in_flight_per_node != fresh.federation.max_in_flight_per_node
    }),
    ("federation.max_node_answer_bytes", |boot, fresh| {
        boot.federation.max_node_answer_bytes != fresh.federation.max_node_answer_bytes
    }),
    ("federation.fan_out_template_upload", |boot, fresh| {
        boot.federation.fan_out_template_upload != fresh.federation.fan_out_template_upload
    }),
    ("federation.fan_out_stored_queries", |boot, fresh| {
        boot.federation.fan_out_stored_queries != fresh.federation.fan_out_stored_queries
    }),
    ("federation.offset", |boot, fresh| {
        boot.federation.offset != fresh.federation.offset
    }),
    ("federation.decomposable_aggregates", |boot, fresh| {
        boot.federation.decomposable != fresh.federation.decomposable
    }),
    ("federation.demographic_endpoint", |boot, fresh| {
        boot.federation.demographic_endpoint != fresh.federation.demographic_endpoint
    }),
    ("stored_queries", stored_queries_changed),
    ("fhir", |boot, fresh| {
        boot.fhir.as_ref().map(|fhir| &fhir.written)
            != fresh.fhir.as_ref().map(|fhir| &fhir.written)
    }),
    ("metrics.listen", |boot, fresh| {
        boot.metrics.listen != fresh.metrics.listen
    }),
    ("metrics.otlp_endpoint", |boot, fresh| {
        boot.metrics.otlp_endpoint != fresh.metrics.otlp_endpoint
    }),
    ("metrics.scrape_token", |boot, fresh| {
        boot.metrics.scrape_token != fresh.metrics.scrape_token
    }),
];

/// The keys a reload does not apply whose value in `fresh` differs from the
/// one the process started with: the core's ([`RESTART_KEYS`]), then each
/// binding's.
fn needs_restart(boot: &Settings, fresh: &Settings) -> Vec<&'static str> {
    RESTART_KEYS
        .iter()
        .filter_map(|(key, changed)| changed(boot, fresh).then_some(*key))
        .chain(
            binding::compiled()
                .iter()
                .flat_map(|binding| binding.needs_restart(boot, fresh)),
        )
        .collect()
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
        _ => binding::class(error).unwrap_or("binding"),
    }
}

/// The ids in `ids`, comma-separated.
fn joined<T>(ids: &[T], name: impl Fn(&T) -> &str) -> String {
    ids.iter().map(name).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
#[cfg(feature = "binding-ihe")]
mod tests {
    use std::collections::BTreeMap;

    use super::{effective, needs_restart};
    use crate::binding::ihe::xcpd::AuditDestination;
    use crate::config::Config;
    use crate::config::settings::Settings;

    /// The settings of a development gateway whose XCPD audit messages go
    /// to `audit`, with the `[xcpd.audit_repository]` keys `repository`.
    fn settings(audit: &str, repository: &str) -> Settings {
        let text = format!(
            "profile = \"development\"\n\n[registry]\ndocument = \"registry.toml\"\n\n[xcpd]\nsender_device = \"2.999.40.1\"\nhome_community = \"2.999.40\"\naudit = \"{audit}\"\n\n[[xcpd.gateway]]\nurl = \"https://xcpd.example.org/rg\"\ndevice = \"2.999.50.1\"\n{repository}"
        );
        Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .expect("the settings resolve")
    }

    #[test]
    fn where_the_audit_messages_go_takes_a_restart() {
        let boot = settings("log", "");
        let fresh = settings(
            "repository",
            "\n[xcpd.audit_repository]\nurl = \"tls://arr.example.org\"\nhostname = \"gateway.example.org\"\n",
        );
        assert_eq!(vec!["xcpd.audit"], needs_restart(&boot, &fresh));
        let applied = effective(&boot, fresh);
        let xcpd = applied.xcpd.expect("the xcpd section reloads");
        assert_eq!(
            AuditDestination::Log,
            xcpd.audit,
            "the boot's destination holds"
        );
        assert_eq!(None, xcpd.audit_repository);

        let same = settings("log", "");
        assert!(needs_restart(&boot, &same).is_empty());
    }

    /// The settings of a development gateway asking the PDQm Supplier at
    /// `url` with `transaction`.
    fn with_pdqm(url: &str, transaction: &str) -> Settings {
        let text = format!(
            "profile = \"development\"\n\n[audit]\ndestination = \"log\"\n\n[pdqm]\nurl = \"{url}\"\ntransaction = \"{transaction}\"\nmaster = \"urn:oid:2.999.1\"\n\n[pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n"
        );
        Config::from_sources(Some(&text), &BTreeMap::new())
            .and_then(|config| config.resolve())
            .expect("the settings resolve")
    }

    #[test]
    fn a_change_to_the_demographics_step_reloads_with_the_resolver() {
        let boot = with_pdqm("https://pdq.example.org/fhir/", "iti-78");
        let fresh = with_pdqm("https://other.example.org/fhir/", "iti-119");
        assert!(
            needs_restart(&boot, &fresh).is_empty(),
            "[pdqm] is reloadable, as [pixm] is"
        );
        let applied = effective(&boot, fresh)
            .pdqm
            .expect("the pdqm section reloads");
        assert_eq!("https://other.example.org/fhir/", applied.url.expose());
    }
}
