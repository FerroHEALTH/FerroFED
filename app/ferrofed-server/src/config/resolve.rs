// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Resolving the configuration tree into the typed settings the run path
//! holds, refusing every bad value under the key that carries it. No
//! specification governs the configuration: our own design.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::time::Duration;

use ferrofed_engine::fanout::Budget;
use ferrofed_registry::id::EndpointId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::OffsetStrategy;
use openehr_federation::id::FederationId;

use crate::base_path::BasePath;
use crate::config::error::Error;
use crate::config::secrets::resolve_credentials;
use crate::config::settings::{
    FederationSettings, MetricsSettings, PixManagerSettings, PixmSettings, ServerSettings,
    Settings, TelemetrySettings,
};
use crate::config::{COMBINING_MARGIN_MS, Config, Metrics, OffsetPaging, Pixm, stored_queries};

impl Config {
    /// Resolves this tree into the settings the run path holds.
    ///
    /// Every `_file` sibling is read here, so a secret reaches the process
    /// once, at boot, and never sits in the configuration tree.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] when a value and its `_file` sibling are
    /// both set, [`Error::Secret`] and [`Error::EmptySecret`] when a `_file`
    /// cannot be read or holds nothing, [`Error::Authorization`] and
    /// [`Error::Basic`] for a credential the `Authorization` header cannot
    /// carry, and the value errors
    /// ([`Error::Listen`], [`Error::BasePath`], [`Error::Zero`], [`Error::Filter`],
    /// [`Error::EndpointId`], [`Error::DemographicEndpoint`], [`Error::Missing`],
    /// [`Error::Scheme`], [`Error::NoScheme`], [`Error::Budget`], [`Error::Url`]),
    /// each naming the key that carries the fault, and
    /// [`Error::StoredQueryFanOutWithoutRegistry`] when definitions would be
    /// distributed with no registry to distribute from. The metrics surface
    /// refuses a remote listener without `metrics.allow_remote`
    /// ([`Error::MetricsRemote`]), a listener on `server.listen`
    /// ([`Error::MetricsShared`]) and a collector that is no `http://` URL
    /// ([`Error::OtlpScheme`]).
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse::<SocketAddr>()
            .map_err(|source| Error::Listen {
                key: String::from("server.listen"),
                source,
            })?;
        let base_path = self
            .server
            .base_path
            .parse::<BasePath>()
            .map_err(|source| Error::BasePath {
                key: String::from("server.base_path"),
                source,
            })?;
        let request_timeout =
            positive_ms("server.request_timeout_ms", self.server.request_timeout_ms)?;
        let shutdown_timeout = positive_ms(
            "server.shutdown_timeout_ms",
            self.server.shutdown_timeout_ms,
        )?;
        if self.server.body_limit_bytes == 0 {
            return Err(Error::Zero {
                key: String::from("server.body_limit_bytes"),
            });
        }
        tracing_subscriber::EnvFilter::try_new(&self.telemetry.filter)
            .map_err(|source| Error::Filter { source })?;
        let mut credentials = BTreeMap::new();
        for (endpoint, section) in &self.credentials {
            let id = EndpointId::new(endpoint.as_str()).map_err(|source| Error::EndpointId {
                key: endpoint.clone(),
                source,
            })?;
            let scheme = resolve_credentials(&format!("credentials.{endpoint}"), section)?;
            credentials.insert(id, scheme);
        }
        let federation = self.resolve_federation(request_timeout)?;
        let pixm = self.pixm.as_ref().map(resolve_pixm).transpose()?;
        let stored_queries = stored_queries::resolve(self)?;
        let metrics = resolve_metrics(&self.metrics, listen)?;
        // NOTE: §12.7 stored-query-fanout, N44: definition fan-out is a facility
        // of the registry and is never offered without it.
        if federation.fan_out_stored_queries && stored_queries.is_none() {
            return Err(Error::StoredQueryFanOutWithoutRegistry);
        }
        Ok(Settings {
            profile: self.profile,
            server: ServerSettings {
                listen,
                base_path,
                request_timeout,
                shutdown_timeout,
                body_limit: self.server.body_limit_bytes,
            },
            telemetry: TelemetrySettings {
                format: self.telemetry.format,
                filter: self.telemetry.filter.clone(),
            },
            registry_document: self.registry.document.clone(),
            registry_format: self.registry.format,
            federation,
            credentials,
            dev: self.dev.clone(),
            pixm,
            stored_queries,
            metrics,
        })
    }

    /// Resolves `[federation]`: both budgets positive (§11.5), and the overall
    /// one plus [`COMBINING_MARGIN_MS`] ending before `request_timeout` when a
    /// registry is configured.
    fn resolve_federation(&self, request_timeout: Duration) -> Result<FederationSettings, Error> {
        let per_node = positive_ms(
            "federation.per_node_timeout_ms",
            self.federation.per_node_timeout_ms,
        )?;
        let overall = positive_ms(
            "federation.overall_timeout_ms",
            self.federation.overall_timeout_ms,
        )?;
        // NOTE: §11.5, the budget only bounds a fan-out, so it is held below the
        // request timeout, by the combining margin, only when the gateway federates.
        if self.registry.document.is_some()
            && request_timeout <= overall.saturating_add(Duration::from_millis(COMBINING_MARGIN_MS))
        {
            return Err(Error::Budget {
                overall_ms: self.federation.overall_timeout_ms,
                margin_ms: COMBINING_MARGIN_MS,
                request_ms: self.server.request_timeout_ms,
            });
        }
        let budget = Budget::new(per_node, overall).map_err(|_zero| Error::Zero {
            key: String::from("federation"),
        })?;
        if let Some(namespace) = &self.federation.default_namespace
            && namespace.is_empty()
        {
            return Err(Error::Missing {
                key: String::from("federation.default_namespace"),
            });
        }
        let named = self.federation.id.as_deref().map(FederationId::new);
        let id = named.transpose().map_err(|_empty| Error::Missing {
            key: String::from("federation.id"),
        })?;
        let binding_ttl = positive_ms("federation.binding_ttl_ms", self.federation.binding_ttl_ms)?;
        let ehr_index_capacity =
            NonZeroU32::new(self.federation.ehr_index_capacity).ok_or_else(|| Error::Zero {
                key: String::from("federation.ehr_index_capacity"),
            })?;
        let max_window =
            NonZeroU32::new(self.federation.max_offset_window).ok_or_else(|| Error::Zero {
                key: String::from("federation.max_offset_window"),
            })?;
        let demographic_endpoint = self
            .federation
            .demographic_endpoint
            .as_deref()
            .map(EndpointId::new)
            .transpose()
            .map_err(|source| Error::DemographicEndpoint { source })?;
        let offset = match self.federation.offset_strategy {
            OffsetPaging::Reject => OffsetStrategy::Reject,
            OffsetPaging::Bounded => OffsetStrategy::Bounded { max_window },
        };
        Ok(FederationSettings {
            id,
            budget,
            default_namespace: self.federation.default_namespace.clone(),
            binding_ttl,
            ehr_index_capacity,
            node_selection: self.federation.node_selection,
            best_effort: self.federation.best_effort,
            offset,
            decomposable: self
                .federation
                .decomposable_aggregates
                .iter()
                .copied()
                .map(AggregateFunction::from)
                .collect(),
            demographic_endpoint,
            fan_out_template_upload: self.federation.fan_out_template_upload,
            fan_out_stored_queries: self.federation.fan_out_stored_queries,
        })
    }
}

/// Resolves `[pixm]`: every Manager URL parses and every secret is read.
fn resolve_pixm(pixm: &Pixm) -> Result<PixmSettings, Error> {
    let mut managers = Vec::with_capacity(pixm.manager.len());
    for (index, manager) in pixm.manager.iter().enumerate() {
        let key = format!("pixm.manager[{index}]");
        let url = url::Url::parse(&manager.url).map_err(|source| Error::Url {
            key: format!("{key}.url"),
            source,
        })?;
        let credentials = manager
            .credentials
            .as_ref()
            .map(|section| resolve_credentials(&format!("{key}.credentials"), section))
            .transpose()?;
        managers.push(PixManagerSettings {
            url,
            members: manager.members.clone(),
            credentials,
        });
    }
    Ok(PixmSettings {
        managers,
        namespaces: pixm.namespaces.clone(),
    })
}

/// Resolves `[metrics]`: the listener on a loopback address unless
/// `allow_remote` is set and never on `server`, the gateway's own address,
/// and the OTLP collector an `http://` URL.
fn resolve_metrics(metrics: &Metrics, server: SocketAddr) -> Result<MetricsSettings, Error> {
    let listen = metrics
        .listen
        .as_deref()
        .map(|listen| {
            listen
                .parse::<SocketAddr>()
                .map_err(|source| Error::Listen {
                    key: String::from("metrics.listen"),
                    source,
                })
        })
        .transpose()?;
    if let Some(address) = listen {
        // NOTE: no specification governs this: our own design; the listener has
        // no authentication, so it stays on the host unless the operator says.
        if !address.ip().is_loopback() && !metrics.allow_remote {
            return Err(Error::MetricsRemote { address });
        }
        if address == server {
            return Err(Error::MetricsShared { address });
        }
    }
    let otlp_endpoint = metrics
        .otlp_endpoint
        .as_deref()
        .map(|endpoint| {
            url::Url::parse(endpoint).map_err(|source| Error::Url {
                key: String::from("metrics.otlp_endpoint"),
                source,
            })
        })
        .transpose()?;
    if otlp_endpoint
        .as_ref()
        .is_some_and(|endpoint| endpoint.scheme() != "http")
    {
        return Err(Error::OtlpScheme);
    }
    Ok(MetricsSettings {
        listen,
        otlp_endpoint,
    })
}

/// Returns the duration `millis` names, refusing zero under `key`.
fn positive_ms(key: &str, millis: u64) -> Result<Duration, Error> {
    if millis == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(Duration::from_millis(millis))
}
