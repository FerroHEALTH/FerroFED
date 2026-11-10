// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! value, so no secret reaches a message, a `Debug` rendering or a log.

/// A configuration the console will not start on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration could not be read: a file that does not read, TOML
    /// that does not parse, an unknown key or variable, or a value of the
    /// wrong type, each named by its key and never by its value.
    #[error("the configuration does not read")]
    Load {
        /// What the family's loader reported.
        #[source]
        source: ferrohealth_config::error::ConfigError,
    },
    /// The configuration sets keys a release retired, each named with its
    /// replacement.
    #[error(transparent)]
    Retired(#[from] ferrohealth_config::removed::RemovedKeys),
    /// A required key is absent or empty.
    #[error("{key} is required")]
    Missing {
        /// The key.
        key: String,
    },
    /// A secret is set inline and through its `_file` sibling, or its file
    /// cannot be read or holds nothing.
    #[error(transparent)]
    SecretFile(#[from] ferrohealth_secret::file::SecretError),
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A locale that is no BCP 47 language tag.
    #[error("{key} is not a language tag")]
    Locale {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: ferrohealth_ui::i18n::UnknownLanguage,
    },
    /// A URL does not parse.
    #[error("{key} is not a URL")]
    Url {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: url::ParseError,
    },
    /// A URL parses but is not one this key admits.
    #[error("{key} {reason}")]
    UrlShape {
        /// The key.
        key: String,
        /// What the URL lacks, as a predicate.
        reason: &'static str,
    },
    /// A replica's name is not one the console admits.
    #[error("{key} {reason}")]
    ReplicaName {
        /// The key.
        key: String,
        /// What is wrong with it, as a predicate.
        reason: &'static str,
    },
    /// A scope list names a scope the console asks for through a setting of
    /// its own.
    #[error("{key} {reason}")]
    Scope {
        /// The key.
        key: String,
        /// What is wrong with it, as a predicate.
        reason: &'static str,
    },
    /// A number is out of its range.
    #[error("{key} must be at least 1")]
    Zero {
        /// The key.
        key: String,
    },
    /// The cookies are set without `Secure` on a console that is not on
    /// loopback.
    #[error(
        "{key} = false is admitted only when oidc.redirect_uri is http on a loopback host (localhost, 127.0.0.0/8 or ::1)"
    )]
    InsecureCookie {
        /// The key.
        key: String,
    },
    /// A refusal of `[oidc]` or `[session]` the keys above do not name, such
    /// as a sealed store with no key.
    #[error(transparent)]
    Session(Box<ferrohealth_session::config::ConfigError>),
}

impl From<ferrohealth_session::config::ConfigError> for Error {
    /// The refusal of a `[oidc]` or `[session]` key, by its key as the
    /// console's own refusals name one.
    fn from(error: ferrohealth_session::config::ConfigError) -> Self {
        use ferrohealth_session::config::ConfigError;
        match error {
            ConfigError::Secret(source) => Self::SecretFile(source),
            ConfigError::Missing { key } => Self::Missing {
                key: key.to_owned(),
            },
            ConfigError::Url { key, source } => Self::Url {
                key: key.to_owned(),
                source,
            },
            ConfigError::Zero { key } => Self::Zero {
                key: key.to_owned(),
            },
            ConfigError::UrlShape { key, reason } => Self::UrlShape {
                key: key.to_owned(),
                reason,
            },
            ConfigError::NoOpenId => Self::Missing {
                key: String::from("oidc.scopes (openid)"),
            },
            ConfigError::OfflineAccessScope => Self::Scope {
                key: String::from("oidc.scopes"),
                reason: "must not list offline_access; set oidc.offline_access = true",
            },
            other => Self::Session(Box::new(other)),
        }
    }
}
