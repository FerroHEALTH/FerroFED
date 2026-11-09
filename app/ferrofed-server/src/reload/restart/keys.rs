// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The core's keys a reload leaves to the next start, each with how a
//! change of it is found, and the test that holds every key of a maximal
//! configuration to being applied by a reload or named as needing a
//! restart. No specification governs this: our own design.

use crate::config::settings::Settings;

/// Whether `fresh` names another stored-query store than the process
/// started with, or sets or unsets one.
fn stored_queries_changed(boot: &Settings, fresh: &Settings) -> bool {
    match (&boot.stored_queries, &fresh.stored_queries) {
        (Some(was), Some(now)) => !was.same_as(now),
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
    }
}

/// Whether `fresh` names another configuration revision store than the
/// process started with, sets or unsets one, or reads it at another interval.
fn revisions_changed(boot: &Settings, fresh: &Settings) -> bool {
    boot.revision_poll != fresh.revision_poll
        || boot.revision_policy != fresh.revision_policy
        || match (&boot.revisions, &fresh.revisions) {
            (Some(was), Some(now)) => !was.same_as(now),
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        }
}

/// Whether an issuer both `boot` and `fresh` trust names other audiences in
/// `fresh`.
fn audience_changed(boot: &Settings, fresh: &Settings) -> bool {
    fresh.server.auth.issuers.iter().any(|issuer| {
        boot.server
            .auth
            .issuers
            .iter()
            .find(|started| started.issuer == issuer.issuer)
            .is_some_and(|started| {
                started.verification.audience() != issuer.verification.audience()
            })
    })
}

/// Whether `fresh` names other signing keys, another JWK Set location,
/// another assertion lifetime or another rotation overlap than the process
/// started with; the keys are compared by `kid`, never by their material.
fn signing_changed(boot: &Settings, fresh: &Settings) -> bool {
    let shape = |settings: &Settings| {
        settings.signing.as_ref().map(|signing| {
            (
                signing.keys.current().kid().to_owned(),
                signing.keys.retiring().map(|key| key.kid().to_owned()),
                signing.keys.next().map(|key| key.kid().to_owned()),
                signing.jwks_uri.as_str().to_owned(),
                signing.rotation_overlap,
            )
        })
    };
    shape(boot) != shape(fresh)
}

/// A key a reload does not apply, with whether its value differs between the
/// settings the process started with and the fresh ones.
pub(super) type RestartKey = (&'static str, fn(&Settings, &Settings) -> bool);

/// The core's keys a reload does not apply, in the order a refusal names
/// them.
pub(super) const RESTART_KEYS: &[RestartKey] = &[
    ("config", revisions_changed),
    ("signing", signing_changed),
    ("audit", |boot, fresh| boot.audit != fresh.audit),
    ("auth.issuer.audience", audience_changed),
    ("server.listen", |boot, fresh| {
        boot.server.listen != fresh.server.listen
    }),
    ("usage_report", |boot, fresh| {
        boot.usage_report != fresh.usage_report
    }),
    ("server.tls", |boot, fresh| {
        boot.server.tls != fresh.server.tls
    }),
    ("gateway.public_url", |boot, fresh| {
        boot.server.public_url != fresh.server.public_url
    }),
    ("server.trusted_proxies", |boot, fresh| {
        boot.server.trusted_proxies != fresh.server.trusted_proxies
    }),
    ("server.forwarded_header", |boot, fresh| {
        boot.server.forwarded_header != fresh.server.forwarded_header
    }),
    ("gateway.replica", |boot, fresh| {
        boot.server.replica != fresh.server.replica
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
    ("server.header_read_timeout_ms", |boot, fresh| {
        boot.server.header_read_timeout_ms != fresh.server.header_read_timeout_ms
    }),
    ("server.idle_timeout_ms", |boot, fresh| {
        boot.server.idle_timeout_ms != fresh.server.idle_timeout_ms
    }),
    ("server.drain_delay_ms", |boot, fresh| {
        boot.server.drain_delay != fresh.server.drain_delay
    }),
    ("server.shutdown_timeout_ms", |boot, fresh| {
        boot.server.shutdown_timeout != fresh.server.shutdown_timeout
    }),
    ("gateway.bindings_drain_timeout_ms", |boot, fresh| {
        boot.server.bindings_drain != fresh.server.bindings_drain
    }),
    ("server.body_limit_bytes", |boot, fresh| {
        boot.server.body_limit != fresh.server.body_limit
    }),
    ("server.compression", |boot, fresh| {
        boot.server.compression != fresh.server.compression
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
    ("telemetry.environment", |boot, fresh| {
        boot.telemetry.environment != fresh.telemetry.environment
    }),
    ("telemetry.otlp", |boot, fresh| {
        boot.telemetry.otlp != fresh.telemetry.otlp
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
    ("federation.max_node_rows", |boot, fresh| {
        boot.federation.max_node_rows != fresh.federation.max_node_rows
    }),
    ("federation.decomposable_aggregates", |boot, fresh| {
        boot.federation.decomposable != fresh.federation.decomposable
    }),
    ("federation.demographic_endpoint", |boot, fresh| {
        boot.federation.demographic_endpoint != fresh.federation.demographic_endpoint
    }),
    ("stored_queries", stored_queries_changed),
    ("store", |boot, fresh| boot.store != fresh.store),
    ("fhir", |boot, fresh| {
        boot.fhir.as_ref().map(|fhir| &fhir.written)
            != fresh.fhir.as_ref().map(|fhir| &fhir.written)
    }),
    ("eehrxf.terminology", |boot, fresh| {
        boot.fhir.as_ref().map(|fhir| &fhir.terminology_written)
            != fresh.fhir.as_ref().map(|fhir| &fhir.terminology_written)
    }),
    ("metrics.listen", |boot, fresh| {
        boot.metrics.listen != fresh.metrics.listen
    }),
    ("metrics.scrape_clients", |boot, fresh| {
        boot.metrics.scrape_clients != fresh.metrics.scrape_clients
    }),
];

#[cfg(test)]
#[cfg(all(feature = "binding-ihe", feature = "binding-nl"))]
mod every_key;
