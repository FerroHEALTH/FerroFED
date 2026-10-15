// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a reload applies and what waits for a restart: the settings a
//! reload builds the federation from, and the changed keys it leaves to the
//! next start. Of `[federation]`, a reload applies the localization policy
//! (`[federation.localization]`) and the consent reporting
//! (`[federation.consent]`). No specification governs this: our own design.

use crate::binding;
use crate::config::settings::Settings;

/// The settings a reload builds the federation from: the sections of `fresh`
/// a reload applies, and every other setting as the process started, each
/// binding keeping the boot's value of what it does not reload.
pub(super) fn effective(boot: &Settings, fresh: Settings) -> Settings {
    let mut federation = boot.federation.clone();
    // NOTE: no specification governs this: our own design; the localization policy
    // applies only beside the node selection and budget it was checked with (§14.1).
    if fresh.federation.node_selection == boot.federation.node_selection
        && fresh.federation.budget == boot.federation.budget
    {
        federation.localization = fresh.federation.localization;
    }
    federation.consent_disclosure = fresh.federation.consent_disclosure;
    federation.consent_emergency = fresh.federation.consent_emergency;
    let mut effective = Settings {
        profile: boot.profile,
        server: boot.server.clone(),
        telemetry: boot.telemetry.clone(),
        federation,
        stored_queries: boot.stored_queries.clone(),
        fhir: boot.fhir.clone(),
        metrics: boot.metrics.clone(),
        signing: boot.signing.clone(),
        audit: boot.audit.clone(),
        revisions: boot.revisions.clone(),
        revision_poll: boot.revision_poll,
        revision_policy: boot.revision_policy.clone(),
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
    ("config", revisions_changed),
    ("signing", signing_changed),
    ("audit", |boot, fresh| boot.audit != fresh.audit),
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
pub(super) fn needs_restart(boot: &Settings, fresh: &Settings) -> Vec<&'static str> {
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

#[cfg(test)]
#[cfg(feature = "binding-ihe")]
mod tests {
    use std::collections::BTreeMap;

    use super::{effective, needs_restart};
    use crate::audit::AuditDestination;
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
