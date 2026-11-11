// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A key 0.0.19 retired was refused naming its replacement for that one
//! release; from 0.0.20 on it is an unknown key, refused by the strict load
//! and naming no replacement (the family's one-release rule of 2026-11-09,
//! `ferrohealth_config::removed`; #1800, #1805, #1806, #1855). No
//! specification governs this: our own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::Config;

/// Each key 0.0.19 retired: the table header that holds it, the key, and the
/// key the refusal names.
const RETIRED: &[(&str, &str, &str)] = &[
    (
        "[federation]",
        "per_node_timeout_ms",
        "federation.per_node_timeout_ms",
    ),
    (
        "[federation]",
        "overall_timeout_ms",
        "federation.overall_timeout_ms",
    ),
    (
        "[federation]",
        "binding_ttl_ms",
        "federation.binding_ttl_ms",
    ),
    (
        "[federation]",
        "max_node_answer_bytes",
        "federation.max_node_answer_bytes",
    ),
    (
        "[federation.localization]",
        "timeout_ms",
        "federation.localization.timeout_ms",
    ),
    (
        "[gateway]",
        "bindings_drain_timeout_ms",
        "gateway.bindings_drain_timeout_ms",
    ),
    (
        "[signing]",
        "node_jwks_cache_s",
        "signing.node_jwks_cache_s",
    ),
    (
        "[signing]",
        "rotation_overlap_s",
        "signing.rotation_overlap_s",
    ),
    ("[emergency]", "window_s", "emergency.window_s"),
    ("[config]", "poll_ms", "config.poll_ms"),
    ("[config]", "confirm_ms", "config.confirm_ms"),
    (
        "[fhir]",
        "document_url_lifetime_secs",
        "fhir.document_url_lifetime_secs",
    ),
    ("[store]", "timeout_ms", "store.timeout_ms"),
    ("[store]", "sweep_interval_s", "store.sweep_interval_s"),
    (
        "[credentials.\"node-a\"]",
        "user",
        "credentials.node-a.user",
    ),
    (
        "[credentials.\"node-a\"]",
        "password",
        "credentials.node-a.password",
    ),
    (
        "[credentials.\"node-a\"]",
        "password_file",
        "credentials.node-a.password_file",
    ),
    (
        "[credentials.\"node-a\"]",
        "oauth",
        "credentials.node-a.oauth",
    ),
    (
        "[credentials.\"node-a\"]",
        "onward",
        "credentials.node-a.onward",
    ),
    ("[eehrxf.terminology]", "url", "eehrxf.terminology.url"),
    (
        "[eehrxf.terminology]",
        "credentials",
        "eehrxf.terminology.credentials",
    ),
    ("[eehrxf.terminology]", "tls", "eehrxf.terminology.tls"),
    (
        "[eehrxf.terminology]",
        "timeout_ms",
        "eehrxf.terminology.timeout_ms",
    ),
    (
        "[audit.repository]",
        "spool_max_bytes",
        "audit.repository.spool_max_bytes",
    ),
    (
        "[audit.repository]",
        "spool_write_timeout_ms",
        "audit.repository.spool_write_timeout_ms",
    ),
    ("[audit.repository]", "url", "audit.repository.url"),
    ("[audit.repository]", "tls", "audit.repository.tls"),
    (
        "[audit.repository]",
        "timeout_ms",
        "audit.repository.timeout_ms",
    ),
    (
        "[audit.repository]",
        "retry_max_ms",
        "audit.repository.retry_max_ms",
    ),
    (
        "[[trust.issuer]]\nissuer = \"https://issuer.example.org\"\n[trust.issuer.patient]",
        "act_is_representative",
        "trust.issuer[0].patient.act_is_representative",
    ),
];

/// The keys of the IHE binding's tables 0.0.19 retired.
#[cfg(feature = "binding-ihe")]
const RETIRED_IHE: &[(&str, &str, &str)] = &[
    (
        "[registry.mcsd]",
        "refresh_interval_s",
        "registry.mcsd.refresh_interval_s",
    ),
    (
        "[registry.mcsd]",
        "deadline_ms",
        "registry.mcsd.deadline_ms",
    ),
    ("[registry.mcsd]", "max_bytes", "registry.mcsd.max_bytes"),
    ("[registry.mcsd]", "url", "registry.mcsd.url"),
    (
        "[registry.mcsd]",
        "credentials",
        "registry.mcsd.credentials",
    ),
    ("[registry.mcsd]", "tls", "registry.mcsd.tls"),
    ("[pdqm]", "timeout_ms", "pdqm.timeout_ms"),
    ("[pdqm]", "url", "pdqm.url"),
    ("[pdqm]", "credentials", "pdqm.credentials"),
    ("[pdqm]", "tls", "pdqm.tls"),
    ("[pmir]", "timeout_ms", "pmir.timeout_ms"),
    ("[pmir]", "check_interval_s", "pmir.check_interval_s"),
    ("[pmir]", "url", "pmir.url"),
    ("[pmir]", "credentials", "pmir.credentials"),
    ("[pmir]", "tls", "pmir.tls"),
    (
        "[xcpd.audit_repository]",
        "spool_max_bytes",
        "xcpd.audit_repository.spool_max_bytes",
    ),
    (
        "[xcpd.audit_repository]",
        "spool_write_timeout_ms",
        "xcpd.audit_repository.spool_write_timeout_ms",
    ),
    (
        "[xcpd.audit_repository]",
        "connect_timeout_ms",
        "xcpd.audit_repository.connect_timeout_ms",
    ),
    (
        "[xcpd.audit_repository]",
        "send_timeout_ms",
        "xcpd.audit_repository.send_timeout_ms",
    ),
    (
        "[xcpd.audit_repository]",
        "retry_max_ms",
        "xcpd.audit_repository.retry_max_ms",
    ),
    ("[xcpd]", "tls", "xcpd.tls"),
    ("[[xcpd.gateway]]", "url", "xcpd.gateway[0].url"),
    ("[[pixm.manager]]", "url", "pixm.manager[0].url"),
    (
        "[[pixm.manager]]",
        "credentials",
        "pixm.manager[0].credentials",
    ),
    ("[[pixm.manager]]", "tls", "pixm.manager[0].tls"),
];

/// The keys of the Dutch binding's tables 0.0.19 retired.
#[cfg(feature = "binding-nl")]
const RETIRED_NL: &[(&str, &str, &str)] = &[
    ("[nl_gf.mitz]", "timeout_ms", "nl_gf.mitz.timeout_ms"),
    ("[nl_gf.mitz]", "url", "nl_gf.mitz.url"),
    ("[nl_gf.mitz]", "credentials", "nl_gf.mitz.credentials"),
    ("[nl_gf.mitz]", "tls", "nl_gf.mitz.tls"),
    ("[nl_gf.nvi]", "url", "nl_gf.nvi.url"),
    ("[nl_gf.nvi]", "credentials", "nl_gf.nvi.credentials"),
    ("[nl_gf.nvi]", "tls", "nl_gf.nvi.tls"),
];

/// Holds every key of `retired` to an unknown-key refusal naming it, with
/// no retired-key refusal.
fn unknown(retired: &[(&str, &str, &str)]) -> Result<(), Box<dyn StdError>> {
    for (header, key, full) in retired {
        let text = format!("{header}\n{key} = 1\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())
            .err()
            .ok_or_else(|| format!("{full} is refused"))?;
        if crate::support::refused_key(&error) != Some(*full) {
            return Err(format!("{full} is an unknown key: {error:?}").into());
        }
    }
    Ok(())
}

#[test]
fn every_key_0_0_19_retired_is_an_unknown_key() -> Result<(), Box<dyn StdError>> {
    unknown(RETIRED)
}

#[cfg(feature = "binding-ihe")]
#[test]
fn every_ihe_key_0_0_19_retired_is_an_unknown_key() -> Result<(), Box<dyn StdError>> {
    unknown(RETIRED_IHE)
}

#[cfg(feature = "binding-nl")]
#[test]
fn every_nl_key_0_0_19_retired_is_an_unknown_key() -> Result<(), Box<dyn StdError>> {
    unknown(RETIRED_NL)
}
