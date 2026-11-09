// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The keys of the gateway's own sections a release retired, refused for
//! the one release that retires them.
//!
//! Each section keeps a retired key as a [`Removed`] field and this module
//! declares it once as a [`Retired`], with the release that retired it.
//! [`check`] refuses every one the sources set, all at once, each naming
//! its replacement. The release after it deletes the field and the
//! declaration, and the strict load refuses the key as unknown. No
//! specification governs this: the family's rule of 2026-11-09, read through
//! `ferrohealth_config::removed`.

use ferrohealth_config::removed::{Removals, Removed, RemovedKeys, Retired};

use crate::config::Config;

/// The release that retired the unit-suffixed keys (#1800).
const UNITS: &str = "0.0.18";

/// A duration such as `30s`.
macro_rules! span {
    ($key:literal) => {
        concat!($key, ", a duration such as `30s` or `5min`")
    };
}

/// An amount of data such as `16MiB`.
macro_rules! size {
    ($key:literal) => {
        concat!($key, ", an amount of data such as `16MiB`")
    };
}

const PER_NODE_TIMEOUT: Retired = Retired::replaced(
    "federation.per_node_timeout_ms",
    span!("federation.per_node_timeout"),
    UNITS,
);
const OVERALL_TIMEOUT: Retired = Retired::replaced(
    "federation.overall_timeout_ms",
    span!("federation.overall_timeout"),
    UNITS,
);
const BINDING_TTL: Retired = Retired::replaced(
    "federation.binding_ttl_ms",
    span!("federation.binding_ttl"),
    UNITS,
);
const MAX_NODE_ANSWER: Retired = Retired::replaced(
    "federation.max_node_answer_bytes",
    size!("federation.max_node_answer_size"),
    UNITS,
);
const LOCALIZATION_TIMEOUT: Retired = Retired::replaced(
    "federation.localization.timeout_ms",
    span!("federation.localization.timeout"),
    UNITS,
);
const BINDINGS_DRAIN: Retired = Retired::replaced(
    "gateway.bindings_drain_timeout_ms",
    span!("gateway.bindings_drain_timeout"),
    UNITS,
);
const NODE_JWKS_CACHE: Retired = Retired::replaced(
    "signing.node_jwks_cache_s",
    span!("signing.node_jwks_cache"),
    UNITS,
);
const ROTATION_OVERLAP: Retired = Retired::replaced(
    "signing.rotation_overlap_s",
    span!("signing.rotation_overlap"),
    UNITS,
);
const EMERGENCY_WINDOW: Retired =
    Retired::replaced("emergency.window_s", span!("emergency.window"), UNITS);
const CONFIG_POLL: Retired = Retired::replaced("config.poll_ms", span!("config.poll"), UNITS);
const CONFIG_CONFIRM: Retired =
    Retired::replaced("config.confirm_ms", span!("config.confirm"), UNITS);
const URL_LIFETIME: Retired = Retired::replaced(
    "fhir.document_url_lifetime_secs",
    span!("fhir.document_url_lifetime"),
    UNITS,
);
const STORE_TIMEOUT: Retired = Retired::replaced("store.timeout_ms", span!("store.timeout"), UNITS);
const STORE_SWEEP: Retired = Retired::replaced(
    "store.sweep_interval_s",
    span!("store.sweep_interval"),
    UNITS,
);
const TERMINOLOGY_TIMEOUT: Retired = Retired::replaced(
    "eehrxf.terminology.timeout_ms",
    span!("eehrxf.terminology.timeout"),
    UNITS,
);
const FEED_SPOOL_SIZE: Retired = Retired::replaced(
    "audit.repository.spool_max_bytes",
    size!("audit.repository.spool_max_size"),
    UNITS,
);
const FEED_SPOOL_WRITE: Retired = Retired::replaced(
    "audit.repository.spool_write_timeout_ms",
    span!("audit.repository.spool_write_timeout"),
    UNITS,
);
const FEED_TIMEOUT: Retired = Retired::replaced(
    "audit.repository.timeout_ms",
    span!("audit.repository.timeout"),
    UNITS,
);
const FEED_RETRY: Retired = Retired::replaced(
    "audit.repository.retry_max_ms",
    span!("audit.repository.retry_max"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const MCSD_REFRESH: Retired = Retired::replaced(
    "registry.mcsd.refresh_interval_s",
    span!("registry.mcsd.refresh_interval"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const MCSD_DEADLINE: Retired = Retired::replaced(
    "registry.mcsd.deadline_ms",
    span!("registry.mcsd.deadline"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const MCSD_SIZE: Retired = Retired::replaced(
    "registry.mcsd.max_bytes",
    size!("registry.mcsd.max_size"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const PDQM_TIMEOUT: Retired = Retired::replaced("pdqm.timeout_ms", span!("pdqm.timeout"), UNITS);
#[cfg(feature = "binding-ihe")]
const PMIR_TIMEOUT: Retired = Retired::replaced("pmir.timeout_ms", span!("pmir.timeout"), UNITS);
#[cfg(feature = "binding-ihe")]
const PMIR_CHECK: Retired =
    Retired::replaced("pmir.check_interval_s", span!("pmir.check_interval"), UNITS);
#[cfg(feature = "binding-ihe")]
const ATNA_SPOOL_SIZE: Retired = Retired::replaced(
    "xcpd.audit_repository.spool_max_bytes",
    size!("xcpd.audit_repository.spool_max_size"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const ATNA_SPOOL_WRITE: Retired = Retired::replaced(
    "xcpd.audit_repository.spool_write_timeout_ms",
    span!("xcpd.audit_repository.spool_write_timeout"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const ATNA_CONNECT: Retired = Retired::replaced(
    "xcpd.audit_repository.connect_timeout_ms",
    span!("xcpd.audit_repository.connect_timeout"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const ATNA_SEND: Retired = Retired::replaced(
    "xcpd.audit_repository.send_timeout_ms",
    span!("xcpd.audit_repository.send_timeout"),
    UNITS,
);
#[cfg(feature = "binding-ihe")]
const ATNA_RETRY: Retired = Retired::replaced(
    "xcpd.audit_repository.retry_max_ms",
    span!("xcpd.audit_repository.retry_max"),
    UNITS,
);
#[cfg(feature = "binding-nl")]
const MITZ_TIMEOUT: Retired =
    Retired::replaced("nl_gf.mitz.timeout_ms", span!("nl_gf.mitz.timeout"), UNITS);

/// Every declaration of this module, for the one-release check.
#[cfg(test)]
const ALL: &[Retired] = &[
    PER_NODE_TIMEOUT,
    OVERALL_TIMEOUT,
    BINDING_TTL,
    MAX_NODE_ANSWER,
    LOCALIZATION_TIMEOUT,
    BINDINGS_DRAIN,
    NODE_JWKS_CACHE,
    ROTATION_OVERLAP,
    EMERGENCY_WINDOW,
    CONFIG_POLL,
    CONFIG_CONFIRM,
    URL_LIFETIME,
    STORE_TIMEOUT,
    STORE_SWEEP,
    TERMINOLOGY_TIMEOUT,
    FEED_SPOOL_SIZE,
    FEED_SPOOL_WRITE,
    FEED_TIMEOUT,
    FEED_RETRY,
    #[cfg(feature = "binding-ihe")]
    MCSD_REFRESH,
    #[cfg(feature = "binding-ihe")]
    MCSD_DEADLINE,
    #[cfg(feature = "binding-ihe")]
    MCSD_SIZE,
    #[cfg(feature = "binding-ihe")]
    PDQM_TIMEOUT,
    #[cfg(feature = "binding-ihe")]
    PMIR_TIMEOUT,
    #[cfg(feature = "binding-ihe")]
    PMIR_CHECK,
    #[cfg(feature = "binding-ihe")]
    ATNA_SPOOL_SIZE,
    #[cfg(feature = "binding-ihe")]
    ATNA_SPOOL_WRITE,
    #[cfg(feature = "binding-ihe")]
    ATNA_CONNECT,
    #[cfg(feature = "binding-ihe")]
    ATNA_SEND,
    #[cfg(feature = "binding-ihe")]
    ATNA_RETRY,
    #[cfg(feature = "binding-nl")]
    MITZ_TIMEOUT,
];

/// Refuses every retired key `config` sets, all at once.
///
/// # Errors
///
/// [`RemovedKeys`] naming each retired key the sources set with its
/// replacement.
pub fn check(config: &Config) -> Result<(), RemovedKeys> {
    let mut removals = Removals::new();
    let mut found = |removed: Removed, retired: &Retired| removals.check(removed, retired);
    let federation = &config.federation;
    found(federation.per_node_timeout_ms, &PER_NODE_TIMEOUT);
    found(federation.overall_timeout_ms, &OVERALL_TIMEOUT);
    found(federation.binding_ttl_ms, &BINDING_TTL);
    found(federation.max_node_answer_bytes, &MAX_NODE_ANSWER);
    if let Some(localization) = &federation.localization {
        found(localization.timeout_ms, &LOCALIZATION_TIMEOUT);
    }
    found(config.gateway.bindings_drain_timeout_ms, &BINDINGS_DRAIN);
    if let Some(signing) = &config.signing {
        found(signing.node_jwks_cache_s, &NODE_JWKS_CACHE);
        found(signing.rotation_overlap_s, &ROTATION_OVERLAP);
    }
    found(config.emergency.window_s, &EMERGENCY_WINDOW);
    found(config.revisions.poll_ms, &CONFIG_POLL);
    found(config.revisions.confirm_ms, &CONFIG_CONFIRM);
    if let Some(fhir) = &config.fhir {
        found(fhir.document_url_lifetime_secs, &URL_LIFETIME);
    }
    if let Some(store) = &config.store {
        found(store.timeout_ms, &STORE_TIMEOUT);
        found(store.sweep_interval_s, &STORE_SWEEP);
    }
    if let Some(terminology) = &config.eehrxf.terminology {
        found(terminology.timeout_ms, &TERMINOLOGY_TIMEOUT);
    }
    if let Some(repository) = &config.audit.repository {
        found(repository.spool_max_bytes, &FEED_SPOOL_SIZE);
        found(repository.spool_write_timeout_ms, &FEED_SPOOL_WRITE);
        found(repository.timeout_ms, &FEED_TIMEOUT);
        found(repository.retry_max_ms, &FEED_RETRY);
    }
    #[cfg(feature = "binding-ihe")]
    if let Some(mcsd) = &config.registry.mcsd {
        found(mcsd.refresh_interval_s, &MCSD_REFRESH);
        found(mcsd.deadline_ms, &MCSD_DEADLINE);
        found(mcsd.max_bytes, &MCSD_SIZE);
    }
    #[cfg(feature = "binding-ihe")]
    {
        if let Some(pdqm) = &config.pdqm {
            found(pdqm.timeout_ms, &PDQM_TIMEOUT);
        }
        if let Some(pmir) = &config.pmir {
            found(pmir.timeout_ms, &PMIR_TIMEOUT);
            found(pmir.check_interval_s, &PMIR_CHECK);
        }
        if let Some(repository) = config
            .xcpd
            .as_ref()
            .and_then(|xcpd| xcpd.audit_repository.as_ref())
        {
            found(repository.spool_max_bytes, &ATNA_SPOOL_SIZE);
            found(repository.spool_write_timeout_ms, &ATNA_SPOOL_WRITE);
            found(repository.connect_timeout_ms, &ATNA_CONNECT);
            found(repository.send_timeout_ms, &ATNA_SEND);
            found(repository.retry_max_ms, &ATNA_RETRY);
        }
    }
    #[cfg(feature = "binding-nl")]
    if let Some(mitz) = config.nl_gf.as_ref().and_then(|nl| nl.mitz.as_ref()) {
        found(mitz.timeout_ms, &MITZ_TIMEOUT);
    }
    removals.finish()
}

#[cfg(test)]
mod tests {
    use super::ALL;

    /// The release the workspace is cut as next: the version a declaration
    /// of this module may name besides the workspace's own.
    fn next(version: &str) -> String {
        let mut parts: Vec<u64> = version
            .split('.')
            .map(|part| part.parse().expect("a release version is numeric"))
            .collect();
        if let Some(patch) = parts.last_mut() {
            *patch += 1;
        }
        parts
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }

    #[test]
    fn every_refusal_lives_for_the_one_release_that_retires_it() {
        // NOTE: no specification governs this: our own design; the workspace version is
        // the last release's until the bump, so a refusal being built names the next one.
        let version = env!("CARGO_PKG_VERSION");
        let upcoming = next(version);
        let stale: Vec<_> = ALL
            .iter()
            .filter(|retired| retired.since() != version && retired.since() != upcoming)
            .map(|retired| (retired.key(), retired.since()))
            .collect();
        assert!(stale.is_empty(), "delete these refusals: {stale:?}");
    }
}
