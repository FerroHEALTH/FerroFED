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

/// The release the workspace is preparing.
///
/// `removed::stale` holds every declaration to it
/// (FerroHEALTH/FerroSYS#339). The workspace version moves only at the cut,
/// so the first pull request after each cut moves this on.
pub const PREPARING: &str = "0.0.19";

/// The release that retired the unit-suffixed keys (#1800).
const UNITS: &str = PREPARING;

/// The release that moved each outbound table under `endpoint` (#1805).
const ENDPOINTS: &str = PREPARING;

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
const ACT_REPRESENTATIVE: Retired = Retired::replaced(
    "act_is_representative",
    "auth.issuer[].act_represents_patient, true on the patients issuer whose act claim names the patient's representative",
    PREPARING,
);
const STORE_TIMEOUT: Retired = Retired::replaced("store.timeout_ms", span!("store.timeout"), UNITS);
const STORE_SWEEP: Retired = Retired::replaced(
    "store.sweep_interval_s",
    span!("store.sweep_interval"),
    UNITS,
);
const NODE_USER: Retired = Retired::replaced(
    "user",
    "credentials.\"<endpoint id>\".credentials.user",
    ENDPOINTS,
);
const NODE_PASSWORD: Retired = Retired::replaced(
    "password",
    "credentials.\"<endpoint id>\".credentials.password",
    ENDPOINTS,
);
const NODE_PASSWORD_FILE: Retired = Retired::replaced(
    "password_file",
    "credentials.\"<endpoint id>\".credentials.password_file",
    ENDPOINTS,
);
const NODE_OAUTH: Retired = Retired::replaced(
    "oauth",
    "credentials.\"<endpoint id>\".credentials.oauth",
    ENDPOINTS,
);
const NODE_ONWARD: Retired = Retired::replaced(
    "onward",
    "credentials.\"<endpoint id>\".credentials.onward",
    ENDPOINTS,
);
const TERMINOLOGY_URL: Retired = Retired::replaced(
    "eehrxf.terminology.url",
    "eehrxf.terminology.endpoint.url",
    ENDPOINTS,
);
const TERMINOLOGY_CREDENTIALS: Retired = Retired::replaced(
    "eehrxf.terminology.credentials",
    "eehrxf.terminology.endpoint.credentials",
    ENDPOINTS,
);
const TERMINOLOGY_TLS: Retired = Retired::replaced(
    "eehrxf.terminology.tls",
    "eehrxf.terminology.endpoint.tls",
    ENDPOINTS,
);
const TERMINOLOGY_TIMEOUT: Retired = Retired::replaced(
    "eehrxf.terminology.timeout_ms",
    span!("eehrxf.terminology.endpoint.timeout"),
    ENDPOINTS,
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
const FEED_URL: Retired = Retired::replaced(
    "audit.repository.url",
    "audit.repository.endpoint.url",
    ENDPOINTS,
);
const FEED_TLS: Retired = Retired::replaced(
    "audit.repository.tls",
    "audit.repository.endpoint.tls",
    ENDPOINTS,
);
const FEED_TIMEOUT: Retired = Retired::replaced(
    "audit.repository.timeout_ms",
    span!("audit.repository.endpoint.timeout"),
    ENDPOINTS,
);
const FEED_RETRY: Retired = Retired::replaced(
    "audit.repository.retry_max_ms",
    span!("audit.repository.endpoint.retry.max_backoff"),
    ENDPOINTS,
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
const PDQM_TIMEOUT: Retired =
    Retired::replaced("pdqm.timeout_ms", span!("pdqm.endpoint.timeout"), UNITS);
#[cfg(feature = "binding-ihe")]
const PMIR_TIMEOUT: Retired =
    Retired::replaced("pmir.timeout_ms", span!("pmir.endpoint.timeout"), UNITS);
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
const MITZ_TIMEOUT: Retired = Retired::replaced(
    "nl_gf.mitz.timeout_ms",
    span!("nl_gf.mitz.endpoint.timeout"),
    UNITS,
);

/// The `url`, `credentials` and `tls` keys of a service table that moved
/// under its `endpoint` table, the replacements written in full.
macro_rules! moved {
    ($url:ident, $credentials:ident, $tls:ident, $table:literal, $feature:literal) => {
        #[cfg(feature = $feature)]
        const $url: Retired = Retired::replaced(
            concat!($table, ".url"),
            concat!($table, ".endpoint.url"),
            ENDPOINTS,
        );
        #[cfg(feature = $feature)]
        const $credentials: Retired = Retired::replaced(
            concat!($table, ".credentials"),
            concat!($table, ".endpoint.credentials"),
            ENDPOINTS,
        );
        #[cfg(feature = $feature)]
        const $tls: Retired = Retired::replaced(
            concat!($table, ".tls"),
            concat!($table, ".endpoint.tls"),
            ENDPOINTS,
        );
    };
}

moved!(PDQM_URL, PDQM_CREDENTIALS, PDQM_TLS, "pdqm", "binding-ihe");
moved!(PMIR_URL, PMIR_CREDENTIALS, PMIR_TLS, "pmir", "binding-ihe");
moved!(
    MCSD_URL,
    MCSD_CREDENTIALS,
    MCSD_TLS,
    "registry.mcsd",
    "binding-ihe"
);
moved!(
    MITZ_URL,
    MITZ_CREDENTIALS,
    MITZ_TLS,
    "nl_gf.mitz",
    "binding-nl"
);
moved!(NVI_URL, NVI_CREDENTIALS, NVI_TLS, "nl_gf.nvi", "binding-nl");
#[cfg(feature = "binding-ihe")]
const PIX_URL: Retired = Retired::replaced("url", "pixm.manager[].endpoint.url", ENDPOINTS);
#[cfg(feature = "binding-ihe")]
const PIX_CREDENTIALS: Retired = Retired::replaced(
    "credentials",
    "pixm.manager[].endpoint.credentials",
    ENDPOINTS,
);
#[cfg(feature = "binding-ihe")]
const PIX_TLS: Retired = Retired::replaced("tls", "pixm.manager[].endpoint.tls", ENDPOINTS);
#[cfg(feature = "binding-ihe")]
const XCPD_TLS: Retired = Retired::replaced("xcpd.tls", "xcpd.gateway[].endpoint.tls", ENDPOINTS);
#[cfg(feature = "binding-ihe")]
const XCPD_GATEWAY_URL: Retired =
    Retired::replaced("url", "xcpd.gateway[].endpoint.url", ENDPOINTS);

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
    ACT_REPRESENTATIVE,
    STORE_TIMEOUT,
    STORE_SWEEP,
    NODE_USER,
    NODE_PASSWORD,
    NODE_PASSWORD_FILE,
    NODE_OAUTH,
    NODE_ONWARD,
    TERMINOLOGY_URL,
    TERMINOLOGY_CREDENTIALS,
    TERMINOLOGY_TLS,
    TERMINOLOGY_TIMEOUT,
    FEED_SPOOL_SIZE,
    FEED_SPOOL_WRITE,
    FEED_URL,
    FEED_TLS,
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
    #[cfg(feature = "binding-ihe")]
    PDQM_URL,
    #[cfg(feature = "binding-ihe")]
    PDQM_CREDENTIALS,
    #[cfg(feature = "binding-ihe")]
    PDQM_TLS,
    #[cfg(feature = "binding-ihe")]
    PMIR_URL,
    #[cfg(feature = "binding-ihe")]
    PMIR_CREDENTIALS,
    #[cfg(feature = "binding-ihe")]
    PMIR_TLS,
    #[cfg(feature = "binding-ihe")]
    MCSD_URL,
    #[cfg(feature = "binding-ihe")]
    MCSD_CREDENTIALS,
    #[cfg(feature = "binding-ihe")]
    MCSD_TLS,
    #[cfg(feature = "binding-ihe")]
    PIX_URL,
    #[cfg(feature = "binding-ihe")]
    PIX_CREDENTIALS,
    #[cfg(feature = "binding-ihe")]
    PIX_TLS,
    #[cfg(feature = "binding-ihe")]
    XCPD_TLS,
    #[cfg(feature = "binding-ihe")]
    XCPD_GATEWAY_URL,
    #[cfg(feature = "binding-nl")]
    MITZ_URL,
    #[cfg(feature = "binding-nl")]
    MITZ_CREDENTIALS,
    #[cfg(feature = "binding-nl")]
    MITZ_TLS,
    #[cfg(feature = "binding-nl")]
    NVI_URL,
    #[cfg(feature = "binding-nl")]
    NVI_CREDENTIALS,
    #[cfg(feature = "binding-nl")]
    NVI_TLS,
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
        found(terminology.url, &TERMINOLOGY_URL);
        found(terminology.credentials, &TERMINOLOGY_CREDENTIALS);
        found(terminology.tls, &TERMINOLOGY_TLS);
        found(terminology.timeout_ms, &TERMINOLOGY_TIMEOUT);
    }
    if let Some(repository) = &config.audit.repository {
        found(repository.spool_max_bytes, &FEED_SPOOL_SIZE);
        found(repository.spool_write_timeout_ms, &FEED_SPOOL_WRITE);
        found(repository.url, &FEED_URL);
        found(repository.tls, &FEED_TLS);
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
    for (id, node) in &config.credentials {
        let table = format!("credentials.{}", toml_key(id));
        for (removed, retired) in [
            (node.user, &NODE_USER),
            (node.password, &NODE_PASSWORD),
            (node.password_file, &NODE_PASSWORD_FILE),
            (node.oauth, &NODE_OAUTH),
            (node.onward, &NODE_ONWARD),
        ] {
            removals.check_under(removed, retired, &table);
        }
    }
    #[cfg(feature = "binding-ihe")]
    ihe_endpoints(config, &mut removals);
    #[cfg(feature = "binding-nl")]
    nl_endpoints(config, &mut removals);
    for (index, issuer) in config.trust.issuer.iter().enumerate() {
        if let Some(patient) = &issuer.patient {
            removals.check_under(
                patient.act_is_representative,
                &ACT_REPRESENTATIVE,
                &format!("trust.issuer[{index}].patient"),
            );
        }
    }
    removals.finish()
}

/// Refuses the keys of the IHE binding's service tables that moved under
/// their `endpoint` table (#1815).
#[cfg(feature = "binding-ihe")]
fn ihe_endpoints(config: &Config, removals: &mut Removals) {
    if let Some(pdqm) = &config.pdqm {
        removals.check(pdqm.url, &PDQM_URL);
        removals.check(pdqm.credentials, &PDQM_CREDENTIALS);
        removals.check(pdqm.tls, &PDQM_TLS);
    }
    if let Some(pmir) = &config.pmir {
        removals.check(pmir.url, &PMIR_URL);
        removals.check(pmir.credentials, &PMIR_CREDENTIALS);
        removals.check(pmir.tls, &PMIR_TLS);
    }
    if let Some(mcsd) = &config.registry.mcsd {
        removals.check(mcsd.url, &MCSD_URL);
        removals.check(mcsd.credentials, &MCSD_CREDENTIALS);
        removals.check(mcsd.tls, &MCSD_TLS);
    }
    if let Some(xcpd) = &config.xcpd {
        removals.check(xcpd.tls, &XCPD_TLS);
        for (index, gateway) in xcpd.gateway.iter().enumerate() {
            removals.check_under(
                gateway.url,
                &XCPD_GATEWAY_URL,
                &format!("xcpd.gateway[{index}]"),
            );
        }
    }
    for (index, manager) in config
        .pixm
        .iter()
        .flat_map(|pixm| pixm.manager.iter())
        .enumerate()
    {
        let table = format!("pixm.manager[{index}]");
        for (removed, retired) in [
            (manager.url, &PIX_URL),
            (manager.credentials, &PIX_CREDENTIALS),
            (manager.tls, &PIX_TLS),
        ] {
            removals.check_under(removed, retired, &table);
        }
    }
}

/// Refuses the keys of the Dutch binding's service tables that moved under
/// their `endpoint` table (#1815).
#[cfg(feature = "binding-nl")]
fn nl_endpoints(config: &Config, removals: &mut Removals) {
    let Some(nl_gf) = &config.nl_gf else {
        return;
    };
    if let Some(mitz) = &nl_gf.mitz {
        removals.check(mitz.url, &MITZ_URL);
        removals.check(mitz.credentials, &MITZ_CREDENTIALS);
        removals.check(mitz.tls, &MITZ_TLS);
    }
    if let Some(nvi) = &nl_gf.nvi {
        removals.check(nvi.url, &NVI_URL);
        removals.check(nvi.credentials, &NVI_CREDENTIALS);
        removals.check(nvi.tls, &NVI_TLS);
    }
}

/// `id` as a TOML key: bare when it is one, quoted otherwise.
fn toml_key(id: &str) -> String {
    let bare = !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if bare {
        id.to_owned()
    } else {
        format!("{id:?}")
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_config::removed::stale;

    use super::{ALL, PREPARING};

    #[test]
    fn every_refusal_lives_for_the_one_release_that_retires_it() {
        let stale: Vec<_> = stale(ALL, PREPARING)
            .into_iter()
            .map(|retired| (retired.key(), retired.since()))
            .collect();
        assert!(stale.is_empty(), "delete these refusals: {stale:?}");
    }
}
