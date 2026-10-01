// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings the run path holds, resolved from the configuration tree.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_engine::fanout::Budget;
use ferrofed_identity::dev::Profile;
use secrecy::SecretString;

use crate::config::DevSection;
use crate::telemetry::Format;

/// The settings the run path holds, with every secret already read.
#[derive(Debug)]
pub struct Settings {
    /// The deployment profile.
    pub profile: Profile,
    /// The HTTP surface.
    pub server: ServerSettings,
    /// The console.
    pub telemetry: TelemetrySettings,
    /// The registry document, when the gateway federates.
    pub registry_document: Option<PathBuf>,
    /// The federated query.
    pub federation: FederationSettings,
    /// The outbound credentials, by endpoint id.
    pub credentials: BTreeMap<String, Scheme>,
    /// The static development cross-reference, as written.
    pub dev: Option<DevSection>,
    /// The PIXm resolver, with every secret read.
    pub pixm: Option<PixmSettings>,
}

/// The PIXm resolver, resolved.
#[derive(Debug)]
pub struct PixmSettings {
    /// The PIX Managers.
    pub managers: Vec<PixManagerSettings>,
    /// A client's issuing namespace mapped to a PIX assigning authority.
    pub namespaces: BTreeMap<String, String>,
}

/// One PIX Manager, resolved.
#[derive(Debug)]
pub struct PixManagerSettings {
    /// The Manager's FHIR base URL.
    pub url: url::Url,
    /// Each member it resolves, mapped to that member's `ehr_id` domain.
    pub members: BTreeMap<String, String>,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
}

/// The federated query, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationSettings {
    /// The per-node timeout and the overall budget of each fan-out.
    pub budget: Budget,
    /// The issuing namespace an unqualified patient identifier resolves in.
    pub default_namespace: Option<String>,
    /// How long the resolution bindings of a client session live.
    pub binding_ttl: Duration,
}

/// The HTTP surface, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// The socket address to bind.
    pub listen: SocketAddr,
    /// How long one request may take before the server answers `408`.
    pub request_timeout: Duration,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout: Duration,
    /// The largest request body the server reads before answering `413`.
    pub body_limit: usize,
}

/// The console, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// The rendering.
    pub format: Format,
    /// The `tracing` filter directive, already known to parse.
    pub filter: String,
}

/// The authentication scheme a credentials section resolves to.
///
/// `Debug` redacts every secret, because [`SecretString`] does.
#[derive(Debug)]
#[non_exhaustive]
pub enum Scheme {
    /// An RFC 6750 bearer token.
    Bearer(SecretString),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
}

impl Settings {
    /// Logs what this process is configured to reach, never a value.
    ///
    /// The line names the endpoints that carry credentials and never the
    /// credentials, so a start-up log states what the process can reach
    /// without stating any of it.
    pub fn log_summary(&self) {
        let endpoints: Vec<&str> = self.credentials.keys().map(String::as_str).collect();
        tracing::info!(
            listen = %self.server.listen,
            profile = ?self.profile,
            registry = self.registry_document.is_some(),
            pix_managers = self.pixm.as_ref().map_or(0, |pixm| pixm.managers.len()),
            credentials = endpoints.join(","),
            "configuration resolved"
        );
    }
}
