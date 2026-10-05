// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a configuration was refused: each refusal names the key, never a
//! secret's value.

use std::path::PathBuf;

/// A configuration the console will not start on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The configuration file could not be read.
    #[error("cannot read the configuration file {}", path.display())]
    Read {
        /// The file named.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// The TOML does not read as the configuration tree.
    #[error("the configuration does not parse")]
    Parse {
        /// What the TOML reader refused.
        #[source]
        source: toml::de::Error,
    },
    /// The merged tree could not be written back for the second read.
    #[error("the configuration with its environment overrides cannot be assembled")]
    Assemble {
        /// What the TOML writer refused.
        #[source]
        source: toml::ser::Error,
    },
    /// An environment override names no section and key.
    #[error("the environment variable {name} names no configuration key")]
    EnvName {
        /// The variable's name.
        name: String,
    },
    /// An environment override names a key under a value that is not a table.
    #[error("the environment variable {name} names a key under a value that is not a table")]
    EnvShape {
        /// The variable's name.
        name: String,
    },
    /// A required key is absent or empty.
    #[error("{key} is required")]
    Missing {
        /// The key.
        key: String,
    },
    /// A secret is set inline and through its `_file` sibling.
    #[error("{key} and {key}_file are both set; set one")]
    Conflict {
        /// The key.
        key: String,
    },
    /// A secret file could not be read.
    #[error("cannot read {key} from {}", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The file it names.
        path: PathBuf,
        /// Why it could not be read.
        #[source]
        source: std::io::Error,
    },
    /// A secret file holds nothing but white space.
    #[error("{key} names {}, which is empty", path.display())]
    EmptySecret {
        /// The `_file` key.
        key: String,
        /// The file it names.
        path: PathBuf,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Address {
        /// The key.
        key: String,
        /// What the parser refused.
        #[source]
        source: std::net::AddrParseError,
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
    /// A number is out of its range.
    #[error("{key} must be at least 1")]
    Zero {
        /// The key.
        key: String,
    },
}
