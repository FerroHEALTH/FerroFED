// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of `[access_log]` and `[audit]`.

/// An `[access_log]` or `[audit]` value the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AccessLogFault {
    /// `[audit] destination = "off"` outside development.
    #[error(
        "{key} = \"off\" records no access and no IHE transaction, which only profile = \"development\" accepts; set it to \"log\" or \"repository\" (Regulation (EU) 2025/327 Annex II 3.2; PIXm §2:3.83.5.1, mCSD §2:3.90.5.1, PMIR §2:3.93.5.1)"
    )]
    FeedAuditOff {
        /// The key that turned the audit off.
        key: String,
    },
    /// `[audit] destination = "log"` outside development while a registry
    /// is configured, whose accesses the log target cannot record.
    #[error(
        "{key} = \"log\" records no caller and no patient, so the access log of a gateway with a registry needs \"repository\" outside profile = \"development\" (Regulation (EU) 2025/327 Annex II 3.2)"
    )]
    AccessAuditLog {
        /// The key that sent the access log to the log target.
        key: String,
    },
    /// `[audit.repository]` is set while the records go elsewhere.
    #[error(
        "[audit.repository] applies only under audit.destination = \"repository\"; remove it, or send the audit records there"
    )]
    FeedAuditRepositoryUnused,
    /// `[access_log]` declares a category map the logging component refuses.
    #[error("[access_log] declares a category map that cannot be used")]
    Map(#[source] ehds_logging::map::MapError),
    /// `[access_log.retention]` declares a retention the logging component refuses.
    #[error("[access_log.retention] declares a retention that cannot be used")]
    Retention(#[source] ehds_logging::retention::RetentionError),
    /// `[access_log.labels]` declares a label the logging component refuses.
    #[error("[access_log.labels] declares a label that cannot be used")]
    Labels(#[source] ehds_logging::label::LabelError),
    /// `[[access_log.emergency_purpose]]` declares a purpose the logging
    /// component refuses.
    #[error("[[access_log.emergency_purpose]] declares a purpose that cannot be used")]
    Emergency(#[source] ehds_logging::emergency::EmergencyError),
    /// `[access_log] patient_namespaces` names an empty namespace.
    #[error("[access_log] patient_namespaces names an empty namespace")]
    Namespace(#[source] ferrofed_identity::role::patient::PatientRefError),
}
