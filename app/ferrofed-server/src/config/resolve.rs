// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Resolving the configuration tree into the typed settings the run path
//! holds, refusing every bad value under the key that carries it. No
//! specification governs the configuration: our own design.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use ferrofed_engine::fanout::Budget;
use ferrofed_identity::profile::Profile;
use ferrofed_registry::id::EndpointId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::OffsetStrategy;
use openehr_federation::id::FederationId;

use crate::config::auth::AuthSettings;
use crate::config::error::Error;
use crate::config::error::credentials::CredentialsFault;
use crate::config::error::federation::FederationFault;
use crate::config::error::server::ServerFault;
use crate::config::grant::GrantFault;
use crate::config::public_url::PublicUrl;
use crate::config::secrets::{resolve_node_credentials, resolve_signing};
use crate::config::server::{ListenerTlsSettings, METRICS_TLS, Metrics, resolve_tls};
use crate::config::settings::{
    ConsentDisclosure, FederationSettings, LocalizationSettings, MetricsSettings, Scheme,
    ServerSettings, Settings, SigningSettings,
};
use crate::config::tls::TlsSettings;
use crate::config::{
    COMBINING_MARGIN_MS, Config, EmergencyConsent, Federation, Localization, NodeSelection,
    OffsetPaging, stored_queries,
};
use ferrohealth_telemetry::section::TelemetrySection;

impl Config {
    /// Resolves this tree into the settings the run path holds.
    ///
    /// Every `_file` sibling is read here, so a secret reaches the process
    /// once, at boot, and never sits in the configuration tree.
    ///
    /// # Errors
    /// Returns [`Error::SecretFile`] when a value and its `_file` sibling are
    /// both set or a `_file` cannot be read or holds nothing, [`CredentialsFault::Authorization`] and
    /// [`CredentialsFault::Basic`] for a credential the `Authorization` header cannot
    /// carry, and the value errors
    /// ([`Error::Listen`], [`ServerFault::BasePath`], [`Error::Zero`],
    /// [`CredentialsFault::EndpointId`], [`FederationFault::DemographicEndpoint`], [`Error::Missing`],
    /// [`CredentialsFault::Scheme`], [`CredentialsFault::NoScheme`], [`ServerFault::Budget`],
    /// [`FederationFault::LocalizationBudget`], [`Error::Url`]),
    /// each naming the key that carries the fault, the stored-query store
    /// errors of [`stored_queries::resolve`], and
    /// [`FederationFault::StoredQueryFanOutWithoutRegistry`] and
    /// [`FederationFault::StoredQueryFanOutReadOnly`] when definitions would be
    /// distributed with no registry, or no `PUT`, to distribute from. The
    /// metrics surface refuses a remote listener without `metrics.allow_remote`
    /// ([`ServerFault::MetricsRemote`]), a listener on the address `server.listen`
    /// binds at a port other than 0 ([`ServerFault::MetricsShared`]), a remote
    /// listener outside the development
    /// profile whose scrape neither a scrape user nor a client CA
    /// authenticates ([`ServerFault::MetricsUnauthenticated`]), and `[telemetry]` refuses a
    /// collector that is no `http://` or `https://` URL ([`ServerFault::OtlpScheme`]) and a
    /// logged query parameter ([`ServerFault::LoggedQueryParameters`]). An OAuth 2.0 grant refuses a missing key, a
    /// scope outside the SMART on openEHR `system` grammar ([`CredentialsFault::Scope`]),
    /// an unusable endpoint or resource ([`CredentialsFault::Grant`]), a PIX Manager
    /// section ([`CredentialsFault::GrantNotHere`]) and a missing `[signing]`
    /// ([`CredentialsFault::GrantWithoutSigning`]); `[signing]` refuses a key that is no
    /// P-256 or P-384 key ([`CredentialsFault::SigningKey`]), an assertion lifetime past five
    /// minutes ([`CredentialsFault::AssertionLifetime`]), an overlap window shorter than
    /// that lifetime plus the nodes' cache time ([`CredentialsFault::RotationOverlap`]),
    /// a next key on another curve than it is meant for
    /// ([`CredentialsFault::NextKeyAlgorithm`]), and a `jwks_uri` that is no `http` or `https` URL ([`Error::HttpUrl`]).
    /// `[auth]` is refused as
    /// [`Auth::resolve`](crate::config::auth::Auth::resolve) refuses it.
    /// `[server.tls]` and `[metrics.tls]` refuse an unset certificate or key
    /// ([`Error::Missing`]), a file that does not read or a key that does not
    /// match its certificate ([`Error::ListenerTls`]), and a healthcheck
    /// identity it never presents ([`ServerFault::HealthcheckIdentityUnused`]).
    /// `gateway.public_url` is refused as
    /// [`PublicUrl::resolve`](crate::config::public_url::PublicUrl::resolve) refuses it,
    /// a `signing.jwks_uri` that names another route under it
    /// ([`ServerFault::PublicUrlDisagrees`]), and an entry of
    /// `server.trusted_proxies` that is no address or CIDR block
    /// ([`ServerFault::TrustedProxy`]). `[fhir]` is refused as
    /// [`Fhir::resolve`](crate::config::fhir::Fhir::resolve) refuses it
    /// ([`Error::Fhir`]).
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self.server.listen;
        let base_path = crate::config::server::resolve_base_path(&self.server)?;
        let public_url = self.gateway.resolve_public_url(&base_path)?;
        let request_timeout =
            positive_ms("server.request_timeout_ms", self.server.request_timeout_ms)?;
        let (shutdown_timeout, bindings_drain) = self.resolve_drain(request_timeout)?;
        if self.server.body_limit_bytes == 0 {
            return Err(Error::Zero {
                key: String::from("server.body_limit_bytes"),
            });
        }
        let telemetry = resolve_telemetry(&self.telemetry)?;
        let (credentials, onward_tls) = self.resolve_credentials()?;
        let signing = self.resolve_signing_keys(public_url.as_ref(), &credentials)?;
        let federation = self.resolve_federation(request_timeout)?;
        let stored_queries = stored_queries::resolve(self)?;
        let metrics = resolve_metrics(&self.metrics, listen, self.profile)?;
        let auth = scrape_users(
            &metrics,
            self.auth.resolve(&self.trust, public_url.as_ref())?,
        )?;
        stored_query_fan_out(&federation, stored_queries.as_ref())?;
        let terminology = self.resolve_terminology()?;
        let mut fhir = self.resolve_fhir(&base_path, public_url.as_ref())?;
        if let Some(fhir) = &mut fhir {
            (fhir.terminology, fhir.validating) =
                crate::config::eehrxf::terminology(terminology.as_ref())?;
            fhir.terminology_written
                .clone_from(&self.eehrxf.terminology);
        }
        let mut settings = Settings {
            profile: self.profile,
            fhir,
            server: ServerSettings {
                listen,
                base_path,
                request_timeout,
                header_read_timeout_ms: self.server.header_read_timeout_ms,
                drain_delay: Duration::from_millis(self.server.drain_delay_ms),
                shutdown_timeout,
                bindings_drain,
                body_limit: self.server.body_limit_bytes,
                auth,
                public_url,
                trusted_proxies: self.server.trusted_proxies.clone(),
                forwarded_header: self.server.forwarded_header,
                overload: crate::config::limits::resolve_overload(&self.server)?,
                tls: crate::config::server::resolve_server_tls(&self.server)?,
                replica: self.gateway.resolve_replica()?,
                compression: crate::config::server::compression(&self.server),
            },
            telemetry,
            registry_document: self.registry.document.clone(),
            registry_format: self.registry.format,
            registry_members: self.registry.console_members()?,
            #[cfg(feature = "binding-ihe")]
            registry_directory: None,
            federation,
            credentials,
            onward_tls,
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
            access_log: crate::access::config::resolve(&self.access_log)?,
            audit: crate::audit::config::AuditSettings::default(),
            revisions: crate::config::revisions::resolve(self)?,
            revision_poll: crate::config::revisions::poll(&self.revisions)?,
            revision_policy: crate::config::revisions::policy(&self.revisions)?,
            usage_report: self.usage_report.clone(),
            deprecated: Vec::new(),
        };
        // NOTE: no specification governs this: our own design; a request is an emergency
        // only by a declared purpose, so passing one to the node needs a purpose declared.
        if settings.federation.consent_emergency == EmergencyConsent::PassToNode
            && settings.access_log.emergency.is_empty()
        {
            return Err(Error::Federation(FederationFault::EmergencyWithoutPurpose));
        }
        let audit = crate::audit::config::resolve(self);
        if let Ok(resolved) = &audit {
            settings.audit.clone_from(resolved);
        }
        for binding in crate::binding::compiled() {
            binding.resolve(self, &mut settings)?;
        }
        // NOTE: no specification governs this: our own design; a binding refuses its own
        // section before the `[audit]` its records and the access records go to.
        audit?;
        Ok(settings)
    }

    /// Resolves `[credentials.<endpoint>]`: the credential each endpoint is
    /// reached with, and the mutual TLS it presents there, by endpoint id.
    ///
    /// # Errors
    ///
    /// [`CredentialsFault::EndpointId`] for a section name that is no endpoint id, and
    /// the errors of [`resolve_node_credentials`].
    fn resolve_credentials(&self) -> Result<OnwardCredentials, Error> {
        let mut credentials = BTreeMap::new();
        let mut onward_tls = BTreeMap::new();
        for (endpoint, section) in &self.credentials {
            let id = EndpointId::new(endpoint.as_str()).map_err(|source| {
                Error::Credentials(CredentialsFault::EndpointId {
                    key: endpoint.clone(),
                    source,
                })
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
        Ok((credentials, onward_tls))
    }

    /// Resolves `server.shutdown_timeout_ms`: `request_timeout` when no
    /// source sets it, and refused when it is shorter, so the drain never
    /// cuts a request the server accepted before its listener closed; and
    /// beside it `gateway.bindings_drain_timeout_ms`, refused at zero.
    fn resolve_drain(&self, request_timeout: Duration) -> Result<(Duration, Duration), Error> {
        let bindings_drain = Duration::from_millis(self.gateway.resolve_bindings_drain()?.get());
        if !self.shutdown_timeout_written {
            return Ok((request_timeout, bindings_drain));
        }
        let shutdown_ms = self.server.shutdown_timeout_ms;
        let shutdown_timeout = positive_ms("server.shutdown_timeout_ms", shutdown_ms)?;
        // NOTE: no specification governs this: our own design; a request accepted
        // just before the listener closes may run its whole request timeout.
        if shutdown_timeout < request_timeout {
            return Err(Error::Server(ServerFault::Drain {
                shutdown_ms,
                request_ms: self.server.request_timeout_ms,
            }));
        }
        Ok((shutdown_timeout, bindings_drain))
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
            return Err(Error::Server(ServerFault::Budget {
                overall_ms: self.federation.overall_timeout_ms,
                margin_ms: COMBINING_MARGIN_MS,
                request_ms: self.server.request_timeout_ms,
            }));
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
        let binding_capacity = positive_count(
            "federation.binding_capacity",
            self.federation.binding_capacity,
        )?;
        let ehr_index_capacity = positive_count(
            "federation.ehr_index_capacity",
            self.federation.ehr_index_capacity,
        )?;
        let max_in_flight_per_node = positive_count(
            "federation.max_in_flight_per_node",
            self.federation.max_in_flight_per_node,
        )?;
        let max_node_answer_bytes = NonZeroUsize::new(self.federation.max_node_answer_bytes)
            .ok_or_else(|| Error::Zero {
                key: String::from("federation.max_node_answer_bytes"),
            })?;
        let (offset, max_node_rows) = resolve_paging(&self.federation)?;
        let demographic_endpoint = self
            .federation
            .demographic_endpoint
            .as_deref()
            .map(EndpointId::new)
            .transpose()
            .map_err(|source| Error::Federation(FederationFault::DemographicEndpoint { source }))?;
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
            max_node_rows,
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
            consent_emergency: self.federation.consent.emergency,
            max_in_flight_per_node,
            max_node_answer_bytes,
        })
    }
}

impl Config {
    /// Resolves `[signing]`, refusing an OAuth 2.0 grant in `credentials`
    /// when it is unset.
    fn resolve_signing_keys(
        &self,
        public: Option<&PublicUrl>,
        credentials: &BTreeMap<EndpointId, Scheme>,
    ) -> Result<Option<SigningSettings>, Error> {
        let signing = self
            .signing
            .as_ref()
            .map(|signing| resolve_signing(signing, public))
            .transpose()?;
        signed_grants(signing.as_ref(), credentials)?;
        Ok(signing)
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
                return Err(Error::Credentials(CredentialsFault::GrantWithoutSigning {
                    section: format!("credentials.{endpoint}.oauth2"),
                }));
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

/// Returns `auth`, refusing a scrape user `[metrics] scrape_users` names
/// that its `[[auth.basic.users]]` does not write.
fn scrape_users(metrics: &MetricsSettings, auth: AuthSettings) -> Result<AuthSettings, Error> {
    match metrics.scrape_users.iter().find(|user| {
        !auth
            .basic
            .as_ref()
            .is_some_and(|basic| basic.names(user.as_str()))
    }) {
        Some(user) => Err(Error::Server(ServerFault::ScrapeUser {
            user: user.clone(),
        })),
        None => Ok(auth),
    }
}

/// Resolves `[metrics]`: the listener on a loopback address unless
/// `allow_remote` is set and never on `server`, the gateway's own address,
/// and its scrape authenticated off loopback outside the development profile.
fn resolve_metrics(
    metrics: &Metrics,
    server: SocketAddr,
    profile: Profile,
) -> Result<MetricsSettings, Error> {
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
    let tls = resolve_tls(metrics.tls.as_ref(), METRICS_TLS)?;
    if let Some(address) = listen {
        // NOTE: no specification governs this: our own design; the listener stays
        // on the host unless the operator says, and off it the scrape is authenticated.
        if !address.ip().is_loopback() && !metrics.allow_remote {
            return Err(Error::Server(ServerFault::MetricsRemote { address }));
        }
        // NOTE: no specification governs this: our own design; port 0 asks the system for
        // a free port at each bind, so two listeners on port 0 never share one.
        if address == server && address.port() != 0 {
            return Err(Error::Server(ServerFault::MetricsShared { address }));
        }
        let mutual = tls
            .as_ref()
            .is_some_and(ListenerTlsSettings::requires_client_certificate);
        if !address.ip().is_loopback()
            && profile == Profile::Production
            && metrics.scrape_users.is_empty()
            && !mutual
        {
            return Err(Error::Server(ServerFault::MetricsUnauthenticated {
                address,
            }));
        }
    }
    Ok(MetricsSettings {
        listen,
        tls,
        scrape_users: metrics.scrape_users.clone(),
    })
}

/// Resolves `[telemetry]`: no logged query parameter beside the paging values
/// the request log carries, and a collector that is an `http://` or
/// `https://` URL.
///
/// The filter parses and the sample ratio is in range once the section is
/// read; the collector's TLS files are read when the export is built.
fn resolve_telemetry(telemetry: &TelemetrySection) -> Result<TelemetrySection, Error> {
    // NOTE: §5.4.3, a query value may carry the patient identifier, so the logged
    // parameters are fixed in code and a deployment cannot widen them.
    if !telemetry.logged_query_parameters.is_empty() {
        return Err(Error::Server(ServerFault::LoggedQueryParameters));
    }
    if let Some(otlp) = &telemetry.otlp
        && !otlp.endpoint.is_empty()
    {
        let parsed = url::Url::parse(otlp.endpoint.expose()).map_err(|source| Error::Url {
            key: String::from("telemetry.otlp.endpoint"),
            source,
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Error::Server(ServerFault::OtlpScheme));
        }
    }
    Ok(telemetry.clone())
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

/// The outbound credentials by endpoint id, and the mutual TLS each endpoint
/// is reached with.
type OnwardCredentials = (
    BTreeMap<EndpointId, Scheme>,
    BTreeMap<EndpointId, TlsSettings>,
);

/// Refuses the stored-query fan-out `federation` asks for unless
/// `stored_queries` is a store the gateway can distribute from.
///
/// # Errors
///
/// [`FederationFault::StoredQueryFanOutWithoutRegistry`] with no store, and
/// [`FederationFault::StoredQueryFanOutReadOnly`] with the read-only files backend.
fn stored_query_fan_out(
    federation: &FederationSettings,
    stored_queries: Option<&stored_queries::Store>,
) -> Result<(), Error> {
    // NOTE: §12.7 stored-query-fanout, N44: definition fan-out is a facility
    // of the registry and is never offered without it.
    if federation.fan_out_stored_queries {
        match stored_queries.map(stored_queries::Store::backend) {
            None => {
                return Err(Error::Federation(
                    FederationFault::StoredQueryFanOutWithoutRegistry,
                ));
            }
            Some(stored_queries::Backend::Files) => {
                return Err(Error::Federation(
                    FederationFault::StoredQueryFanOutReadOnly,
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
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

/// The `OFFSET` strategy with its bound (§11.6.2, N39), and the most rows
/// read of one node for a query with no `LIMIT` (§11.5), as `federation`
/// sets them; a zero bound or ceiling is refused.
fn resolve_paging(federation: &Federation) -> Result<(OffsetStrategy, NonZeroU32), Error> {
    let max_window = positive_count("federation.max_offset_window", federation.max_offset_window)?;
    let max_node_rows = positive_count("federation.max_node_rows", federation.max_node_rows)?;
    let offset = match federation.offset_strategy {
        OffsetPaging::Reject => OffsetStrategy::Reject,
        OffsetPaging::Bounded => OffsetStrategy::Bounded { max_window },
    };
    Ok((offset, max_node_rows))
}

/// The count `key` sets, refused when it is zero.
fn positive_count(key: &str, count: u32) -> Result<NonZeroU32, Error> {
    NonZeroU32::new(count).ok_or_else(|| Error::Zero {
        key: key.to_owned(),
    })
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
        return Err(Error::Federation(FederationFault::LocalizationBudget {
            timeout_ms: section.timeout_ms,
            overall_ms: federation.overall_timeout_ms,
        }));
    }
    Ok(LocalizationSettings {
        on_failure: section.on_failure,
        timeout,
    })
}
