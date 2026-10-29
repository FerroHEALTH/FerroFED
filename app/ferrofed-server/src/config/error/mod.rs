// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of a configuration the server does not start on.

use std::path::PathBuf;

use ferrofed_registry::error::IdError;

use crate::config::error::access_log::AccessLogFault;
use crate::config::error::binding::BindingFault;
use crate::config::error::credentials::CredentialsFault;
use crate::config::error::federation::FederationFault;
use crate::config::error::server::ServerFault;
use crate::config::transport::{CleartextError, TrustAnchorError};

pub mod access_log;
pub mod basic;
pub mod binding;
pub mod credentials;
pub mod federation;
pub mod server;

/// A configuration the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The file, the environment or a `--set` pair does not read as the
    /// configuration tree: a file that cannot be read, text that is not
    /// TOML, an unknown key, or a value of the wrong type.
    ///
    /// The refusal of `ferrohealth-config` names the key and the variable
    /// that set it, and quotes no value, so a refused line of
    /// `[credentials]` never shows its secret (§5.4.1, N33).
    #[error("{}", Refused(file.as_deref()))]
    Load {
        /// The file the refused text was read from, when it came from one.
        file: Option<PathBuf>,
        /// What the loader refused.
        #[source]
        source: ferrohealth_config::error::ConfigError,
    },
    /// The configuration sets `[dev]`, the static cross-reference FerroFED
    /// no longer has: it keeps no patient index (#1377).
    #[error(
        "[dev] is no longer read: FerroFED keeps no patient index, so it resolves every patient through a PIX Manager; remove [dev] and configure [[pixm.manager]]"
    )]
    DevRemoved,
    /// A secret and its `_file` sibling are both set, or the file cannot be
    /// read or holds nothing.
    #[error(transparent)]
    SecretFile(#[from] ferrohealth_secret::file::SecretError),
    /// A file a key names (a PEM bundle, a key) could not be read.
    #[error("{key} names {}, which could not be read", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A key a section needs is not set.
    #[error("{key} is not set, and its section needs it")]
    Missing {
        /// The key that carries no value.
        key: String,
    },
    /// `[auth]` is refused: the key and why, never a value it holds.
    #[error("{key} {fault}")]
    Auth {
        /// The key that carries the fault.
        key: String,
        /// Why it is refused.
        fault: crate::config::auth::AuthFault,
    },
    /// A section `ferrohealth-smart` reads is refused: `[auth]`, or an
    /// `oauth` credentials table.
    #[error("{section} is refused")]
    Smart {
        /// The section.
        section: String,
        /// What the crate refused, naming the key and never a value.
        #[source]
        source: Box<ferrohealth_smart::config::ConfigError>,
    },
    /// A URL does not parse.
    #[error("{key} is not a URL")]
    Url {
        /// The key that holds it.
        key: String,
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// A URL carries a user name or a password in its userinfo, where the
    /// credentials belong in their own section.
    #[error("{key} carries a user name or password; set them in {section} instead")]
    UrlCredentials {
        /// The key that holds the URL.
        key: String,
        /// The section the credentials belong in.
        section: String,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Listen {
        /// The key that holds it.
        key: String,
        /// What the address parser reported.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A duration or a size that must be positive is zero.
    #[error("{key} is zero; it must be positive")]
    Zero {
        /// The key that holds it.
        key: String,
    },
    /// A `[trust.issuer.patient]` binding names something that is not an
    /// endpoint id.
    #[error("{key} is not an endpoint id")]
    PatientEndpoint {
        /// The key that carries it.
        key: String,
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// A URL that must be absolute `http` or `https` is not.
    #[error("{key} must be an http or https URL with no user name or password")]
    HttpUrl {
        /// The key that holds it.
        key: String,
    },
    /// A URL that carries a patient identifier or a credential is not
    /// `https`, outside a configuration marked for development.
    #[error(transparent)]
    Cleartext(#[from] CleartextError),
    /// A FAPI 2.0 grant, or an `oauth2` grant's assertion audience, is
    /// refused.
    #[error(transparent)]
    GrantFault(#[from] crate::config::grant::GrantFault),
    /// TLS material, or a use of mutual TLS, is refused (RFC 8705).
    #[error(transparent)]
    TlsFault(#[from] crate::config::tls::TlsFault),
    /// The certificate, the key or the client CA of a listener's TLS cannot
    /// be served with.
    #[error("{table} cannot be served with")]
    ListenerTls {
        /// The listener's table, `server.tls` or `metrics.tls`.
        table: &'static str,
        /// What the server reported, naming the key at fault.
        #[source]
        source: ferrohealth_server::tls::TlsError,
    },
    /// A URL the gateway verifies its callers against is plain `http` to a
    /// host that is not loopback.
    #[error(transparent)]
    TrustAnchor(#[from] TrustAnchorError),
    /// The `[fhir]` face is refused.
    #[error(transparent)]
    Fhir(#[from] crate::config::fhir::FhirError),
    /// `[eehrxf.terminology]` is refused.
    #[error(transparent)]
    Terminology(#[from] crate::config::eehrxf::TerminologyFault),
    /// `[config]` is refused.
    #[error(transparent)]
    Revisions(#[from] crate::config::revisions::RevisionsFault),
    /// `[server]`, `[metrics]` or `[telemetry]` is refused.
    #[error(transparent)]
    Server(#[from] ServerFault),
    /// `[federation]`, `[stored_queries]` or `[registry]` is refused.
    #[error(transparent)]
    Federation(#[from] FederationFault),
    /// A credential, a signing key or a grant is refused.
    #[error(transparent)]
    Credentials(#[from] CredentialsFault),
    /// An identity binding's section is refused.
    #[error(transparent)]
    Binding(#[from] BindingFault),
    /// `[access_log]` or `[audit]` is refused.
    #[error(transparent)]
    AccessLog(#[from] AccessLogFault),
}

impl Error {
    /// The refusal of a configuration table that cannot be written as TOML,
    /// such as a revision's.
    pub(crate) const fn assemble(source: toml::ser::Error) -> Self {
        Self::Load {
            file: None,
            source: ferrohealth_config::error::ConfigError::Assemble { source },
        }
    }
}

/// The message of [`Error::Load`]: the file it names, when it names one.
struct Refused<'a>(Option<&'a std::path::Path>);

impl std::fmt::Display for Refused<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(file) => write!(f, "the configuration file {} is refused", file.display()),
            None => f.write_str("the configuration is refused"),
        }
    }
}
