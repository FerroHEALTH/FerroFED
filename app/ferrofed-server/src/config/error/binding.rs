// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of the identity bindings and the development cross-reference.

/// An identity binding's value the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingFault {
    /// The `[dev]` table does not have the shape of `[[dev.crossref]]` rows.
    ///
    /// The reader's own message is not kept: it may quote a row's value, and
    /// a row's value is a patient identifier.
    #[error(
        "the [dev] table is not valid: every [[dev.crossref]] row names namespace, value, member and ehr_id, and nothing else"
    )]
    DevTable,
    /// Audit messages are turned off outside a configuration marked for
    /// development.
    #[error(
        "{key} = \"off\" records no ITI-55 audit message, which only profile = \"development\" accepts; set it to \"log\" (ITI TF-2 §3.55.5.1)"
    )]
    #[cfg(feature = "binding-ihe")]
    AuditOff {
        /// The key that turned the audit off.
        key: String,
    },
    /// `[xcpd.audit_repository]` is set while the audit messages go
    /// elsewhere.
    #[error(
        "[xcpd.audit_repository] applies only under xcpd.audit = \"repository\"; remove it, or send the audit messages there"
    )]
    #[cfg(feature = "binding-ihe")]
    AuditRepositoryUnused,
    /// `nl_gf.nvi.namespaces` lists a BSN system as standing for the
    /// pseudonymised BSN, which would send a BSN to the NVI.
    #[error(
        "nl_gf.nvi.namespaces lists {namespace}, a BSN system: the NVI is keyed on the pseudonymised BSN, and the gateway never sends it a BSN"
    )]
    #[cfg(feature = "binding-nl")]
    BsnAsPseudonym {
        /// The namespace as written, a naming system and never a value.
        namespace: String,
    },
    /// `nl_gf.nvi.credentials` names an OAuth 2.0 or FAPI 2.0 grant, which
    /// the Generic Functions IG defines for no data user of the Localization
    /// Service.
    #[error(
        "{key} is not a credential the NVI takes: nl_gf.nvi.credentials takes a bearer token, basic credentials or the nuts grant of GF-Authentication"
    )]
    #[cfg(feature = "binding-nl")]
    NviGrant {
        /// The key of the refused table, such as
        /// `nl_gf.nvi.credentials.oauth2`.
        key: String,
    },
    /// A `[nl_gf.mitz]` value the closed authorization question does not
    /// take: the key and why, never a patient value.
    #[error("{key} {fault}")]
    #[cfg(feature = "binding-nl")]
    Mitz {
        /// The key.
        key: String,
        /// Why it is refused.
        fault: &'static str,
    },
    /// A `[pdqm]` value the demographics step does not take: the key and
    /// why, never a patient value.
    #[error("{key} {fault}")]
    #[cfg(feature = "binding-ihe")]
    Pdqm {
        /// The key.
        key: String,
        /// Why it is refused.
        fault: &'static str,
    },
    /// A syslog header value is empty, too long, or holds a character
    /// syslog cannot carry (RFC 5424 §6).
    #[error("{key} cannot be a syslog header field")]
    #[cfg(feature = "binding-ihe")]
    SyslogHeader {
        /// The key that holds it.
        key: String,
        /// What the syslog writer reported.
        #[source]
        source: ihe_atna::atna::syslog::HeaderError,
    },
    /// The path the ITI-93 feed is served at is not a path of its own: it
    /// must start with `/`, carry no query or fragment, and stay off the
    /// ITS-REST surface, the health family and the well-known documents.
    #[error(
        "{key} must be a path that starts with /, carries no query or fragment, and is not under /v1, /health or /.well-known"
    )]
    #[cfg(feature = "binding-ihe")]
    FeedPath {
        /// The key that carries the path.
        key: String,
    },
    /// The demographics step's budget and the localizer's do not end before
    /// the overall budget, of which each is a part, so the two could leave
    /// no time to resolve and ask the members (§11.5; no specification
    /// governs the step's budget: our own design).
    #[error(
        "pdqm.timeout_ms ({timeout_ms}) plus federation.localization.timeout_ms ({localization_ms}) must be below federation.overall_timeout_ms ({overall_ms}), of which both are a part (§11.5)"
    )]
    #[cfg(feature = "binding-ihe")]
    DemographicsBudget {
        /// The demographics step's budget.
        timeout_ms: u64,
        /// The localizer's budget, zero with no localizer.
        localization_ms: u64,
        /// The overall fan-out budget.
        overall_ms: u64,
    },
    /// The consent pre-filter's budget, the demographics step's and the
    /// localizer's do not end before the overall budget, of which each is a
    /// part, so the three could leave no time to resolve and ask the members
    /// (§11.5; no specification governs the pre-filter's budget: our own
    /// design).
    #[error(
        "nl_gf.mitz.timeout_ms ({timeout_ms}) plus pdqm.timeout_ms ({demographics_ms}) plus federation.localization.timeout_ms ({localization_ms}) must be below federation.overall_timeout_ms ({overall_ms}), of which all three are a part (§11.5)"
    )]
    #[cfg(feature = "binding-nl")]
    PrefilterBudget {
        /// The consent pre-filter's budget.
        timeout_ms: u64,
        /// The demographics step's budget, zero with no `[pdqm]`.
        demographics_ms: u64,
        /// The localizer's budget, zero with no localizer.
        localization_ms: u64,
        /// The overall fan-out budget.
        overall_ms: u64,
    },
}
