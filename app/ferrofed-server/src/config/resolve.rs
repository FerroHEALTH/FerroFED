// SPDX-FileCopyrightText: Cadasto B.V.
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
use ferrofed_registry::secret::SecretUrl;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::OffsetStrategy;
use openehr_federation::id::FederationId;

use crate::base_path::BasePath;
use crate::config::error::Error;
use crate::config::grant::GrantFault;
use crate::config::secrets::{resolve_node_credentials, resolve_signing};
use crate::config::settings::{
    ConsentDisclosure, FederationSettings, LocalizationSettings, MetricsSettings, Scheme,
    ServerSettings, Settings, SigningSettings, TelemetrySettings,
};
use crate::config::{
    COMBINING_MARGIN_MS, Config, Federation, Localization, Metrics, NodeSelection, OffsetPaging,
    Telemetry, stored_queries,
};
use crate::telemetry::SampleRatio;

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
    /// [`Error::Scheme`], [`Error::NoScheme`], [`Error::Budget`],
    /// [`Error::LocalizationBudget`], [`Error::Url`]),
    /// each naming the key that carries the fault, the stored-query store
    /// errors of [`stored_queries::resolve`], and
    /// [`Error::StoredQueryFanOutWithoutRegistry`] and
    /// [`Error::StoredQueryFanOutReadOnly`] when definitions would be
    /// distributed with no registry, or no `PUT`, to distribute from. The
    /// metrics surface refuses a remote listener without `metrics.allow_remote`
    /// ([`Error::MetricsRemote`]), a listener on `server.listen`
    /// ([`Error::MetricsShared`]), and the metrics push and the trace export
    /// each refuse a collector that is no `http://` URL
    /// ([`Error::OtlpScheme`]). An OAuth 2.0 grant refuses a missing key, a
    /// scope outside the SMART on openEHR `system` grammar ([`Error::Scope`]),
    /// an unusable endpoint or resource ([`Error::Grant`]), a PIX Manager
    /// section ([`Error::GrantNotHere`]) and a missing `[signing]`
    /// ([`Error::GrantWithoutSigning`]); `[signing]` refuses a key that is no
    /// P-256 or P-384 key ([`Error::SigningKey`]), an assertion lifetime past five
    /// minutes ([`Error::AssertionLifetime`]), an overlap window shorter than
    /// that lifetime plus the nodes' cache time ([`Error::RotationOverlap`]),
    /// a next key on another curve than it is meant for
    /// ([`Error::NextKeyAlgorithm`]), and a `jwks_uri` that is no `http` or `https` URL ([`Error::HttpUrl`]).
    /// `[auth]` is refused as
    /// [`Auth::resolve`](crate::config::auth::Auth::resolve) refuses it.
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
        let shutdown_timeout = self.resolve_drain(request_timeout)?;
        if self.server.body_limit_bytes == 0 {
            return Err(Error::Zero {
                key: String::from("server.body_limit_bytes"),
            });
        }
        let telemetry = resolve_telemetry(&self.telemetry)?;
        let mut credentials = BTreeMap::new();
        let mut onward_tls = BTreeMap::new();
        for (endpoint, section) in &self.credentials {
            let id = EndpointId::new(endpoint.as_str()).map_err(|source| Error::EndpointId {
                key: endpoint.clone(),
                source,
            })?;
            let (scheme, tls) =
                resolve_node_credentials(&format!("credentials.{endpoint}"), section)?;
            if let Some(tls) = tls {
                onward_tls.insert(id.clone(), tls);
            }
            if let Some(scheme) = scheme {
                credentials.insert(id, scheme);
            }
        }
        let signing = self.signing.as_ref().map(resolve_signing).transpose()?;
        signed_grants(signing.as_ref(), &credentials)?;
        let federation = self.resolve_federation(request_timeout)?;
        let stored_queries = stored_queries::resolve(self)?;
        let metrics = resolve_metrics(&self.metrics, listen)?;
        // NOTE: §12.7 stored-query-fanout, N44: definition fan-out is a facility
        // of the registry and is never offered without it.
        if federation.fan_out_stored_queries {
            match stored_queries.as_ref().map(stored_queries::Store::backend) {
                None => return Err(Error::StoredQueryFanOutWithoutRegistry),
                Some(stored_queries::Backend::Files) => {
                    return Err(Error::StoredQueryFanOutReadOnly);
                }
                Some(_) => {}
            }
        }
        let mut settings = Settings {
            profile: self.profile,
            server: ServerSettings {
                listen,
                base_path,
                request_timeout,
                drain_delay: Duration::from_millis(self.server.drain_delay_ms),
                shutdown_timeout,
                body_limit: self.server.body_limit_bytes,
                auth: self.auth.resolve()?,
                overload: self.server.resolve_overload()?,
            },
            telemetry,
            registry_document: self.registry.document.clone(),
            registry_format: self.registry.format,
            #[cfg(feature = "binding-ihe")]
            registry_directory: None,
            federation,
            credentials,
            onward_tls,
            dev: None,
            #[cfg(feature = "binding-ihe")]
            pixm: None,
            #[cfg(feature = "binding-ihe")]
            xcpd: None,
            #[cfg(feature = "binding-nl")]
            nl_gf: None,
            #[cfg(feature = "binding-ihe")]
            pmir: None,
            #[cfg(feature = "binding-ihe")]
            pdqm: None,
            stored_queries,
            metrics,
            signing,
            #[cfg(feature = "binding-ihe")]
            audit: crate::binding::ihe::audit::config::AuditSettings::default(),
        };
        for binding in crate::binding::compiled() {
            binding.resolve(self, &mut settings)?;
        }
        Ok(settings)
    }

    /// Resolves `server.shutdown_timeout_ms`: `request_timeout` when unset,
    /// and refused when it is shorter, so the drain never cuts a request the
    /// server accepted before its listener closed.
    fn resolve_drain(&self, request_timeout: Duration) -> Result<Duration, Error> {
        let Some(shutdown_ms) = self.server.shutdown_timeout_ms else {
            return Ok(request_timeout);
        };
        let shutdown_timeout = positive_ms("server.shutdown_timeout_ms", shutdown_ms)?;
        // NOTE: no specification governs this: our own design; a request accepted
        // just before the listener closes may run its whole request timeout.
        if shutdown_timeout < request_timeout {
            return Err(Error::Drain {
                shutdown_ms,
                request_ms: self.server.request_timeout_ms,
            });
        }
        Ok(shutdown_timeout)
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
        if self.registry.configured()
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
        let binding_capacity =
            NonZeroU32::new(self.federation.binding_capacity).ok_or_else(|| Error::Zero {
                key: String::from("federation.binding_capacity"),
            })?;
        let ehr_index_capacity =
            NonZeroU32::new(self.federation.ehr_index_capacity).ok_or_else(|| Error::Zero {
                key: String::from("federation.ehr_index_capacity"),
            })?;
        let max_in_flight_per_node = NonZeroU32::new(self.federation.max_in_flight_per_node)
            .ok_or_else(|| Error::Zero {
                key: String::from("federation.max_in_flight_per_node"),
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
        let localization = match (
            &self.federation.localization,
            self.federation.node_selection,
        ) {
            (Some(section), _) => Some(resolve_localization(section, &self.federation)?),
            (None, Some(NodeSelection::Localized)) => Some(resolve_localization(
                &Localization::default(),
                &self.federation,
            )?),
            (None, _) => None,
        };
        Ok(FederationSettings {
            localization,
            id,
            budget,
            default_namespace: self.federation.default_namespace.clone(),
            binding_ttl,
            binding_capacity,
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
            consent_disclosure: ConsentDisclosure::of(self.federation.consent.disclose),
            max_in_flight_per_node,
        })
    }
}

/// Refuses an OAuth 2.0 grant in `credentials` when `signing` is unset.
fn signed_grants(
    signing: Option<&SigningSettings>,
    credentials: &BTreeMap<EndpointId, Scheme>,
) -> Result<(), Error> {
    // NOTE: §13.1, N25: the client assertion of every grant is signed with the
    // gateway's key, so a grant without one is refused at load.
    if signing.is_some() {
        return Ok(());
    }
    for (endpoint, scheme) in credentials {
        match scheme {
            Scheme::OAuth2(_) => {
                return Err(Error::GrantWithoutSigning {
                    section: format!("credentials.{endpoint}.oauth2"),
                });
            }
            // NOTE: FAPI 2.0 Security Profile §5.4.2, the client key is published as a JWK
            // Set, which the gateway serves only beside its [signing] keys.
            Scheme::Fapi2(_) => {
                return Err(GrantFault::WithoutSigning {
                    section: format!("credentials.{endpoint}.fapi2"),
                }
                .into());
            }
            _ => {}
        }
    }
    Ok(())
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
    Ok(MetricsSettings {
        listen,
        otlp_endpoint: otlp_collector("metrics.otlp_endpoint", metrics.otlp_endpoint.as_ref())?,
    })
}

/// Resolves `[telemetry]`: a filter that parses, a trace collector that is
/// an `http://` URL, and a sample ratio from `0.0` to `1.0`.
fn resolve_telemetry(telemetry: &Telemetry) -> Result<TelemetrySettings, Error> {
    tracing_subscriber::EnvFilter::try_new(&telemetry.filter)
        .map_err(|source| Error::Filter { source })?;
    let trace_sample_ratio =
        SampleRatio::new(telemetry.trace_sample_ratio).ok_or(Error::SampleRatio {
            value: telemetry.trace_sample_ratio,
        })?;
    Ok(TelemetrySettings {
        format: telemetry.format,
        filter: telemetry.filter.clone(),
        otlp_endpoint: otlp_collector("telemetry.otlp_endpoint", telemetry.otlp_endpoint.as_ref())?,
        trace_sample_ratio,
    })
}

/// Resolves the OTLP collector `endpoint` under `key`: an `http://` URL,
/// because the exporters speak gRPC without TLS, to a collector beside the
/// gateway.
fn otlp_collector(key: &str, endpoint: Option<&SecretUrl>) -> Result<Option<SecretUrl>, Error> {
    let Some(endpoint) = endpoint else {
        return Ok(None);
    };
    let parsed = url::Url::parse(endpoint.expose()).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    if parsed.scheme() != "http" {
        return Err(Error::OtlpScheme {
            key: key.to_owned(),
        });
    }
    Ok(Some(SecretUrl::new(String::from(parsed))))
}

/// Returns `count`, refusing zero under `key`.
#[cfg(feature = "binding-ihe")]
pub(crate) fn positive(key: &str, count: usize) -> Result<usize, Error> {
    if count == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(count)
}

/// Returns the duration `millis` names, refusing zero under `key`.
pub(crate) fn positive_ms(key: &str, millis: u64) -> Result<Duration, Error> {
    if millis == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(Duration::from_millis(millis))
}

/// The localizer's budget the configuration declares, in milliseconds: the
/// `[federation.localization]` budget when the section is written, its
/// default under `node_selection = "localized"` without it, and zero with no
/// localizer, as `resolve_federation` resolves it.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub(crate) fn localization_budget_ms(config: &Config) -> u64 {
    match (
        &config.federation.localization,
        config.federation.node_selection,
    ) {
        (Some(section), _) => section.timeout_ms,
        (None, Some(NodeSelection::Localized)) => Localization::default().timeout_ms,
        (None, _) => 0,
    }
}

/// Resolves `[federation.localization]`: a positive budget that ends before
/// the overall one, of which it is a part (§11.5, §14.1).
fn resolve_localization(
    section: &Localization,
    federation: &Federation,
) -> Result<LocalizationSettings, Error> {
    let timeout = positive_ms("federation.localization.timeout_ms", section.timeout_ms)?;
    if section.timeout_ms >= federation.overall_timeout_ms {
        return Err(Error::LocalizationBudget {
            timeout_ms: section.timeout_ms,
            overall_ms: federation.overall_timeout_ms,
        });
    }
    Ok(LocalizationSettings {
        on_failure: section.on_failure,
        timeout,
    })
}
