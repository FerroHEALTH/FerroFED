// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[gateway]`: the gateway's own keys beside the shared `[server]`.
//!
//! The URL clients reach it at, the name of this process, and how long the
//! bindings may take to stop. No specification governs the configuration:
//! our own design.

use std::num::NonZeroU64;

use ferrofed_registry::replica::Replica;
use serde::Deserialize;

use crate::base_path::BasePath;
use crate::config::error::Error;
use crate::config::error::server::ServerFault;
use crate::config::public_url::PublicUrl;

/// The environment variable a process's host name is read from when
/// `gateway.replica` names none; Kubernetes sets it to the pod's name.
pub const HOST_NAME_ENV: &str = "HOSTNAME";

/// The gateway's own keys.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gateway {
    /// The absolute URL clients reach `{base}` at, such as
    /// `https://gateway.example.org/fed`, whose path is `server.base_path`.
    /// Set, `auth.audience`, `signing.jwks_uri` and `pmir.callback_url`
    /// default from it, and a `signing.jwks_uri` or `pmir.callback_url` that
    /// names another route than the gateway serves under it is refused
    /// ([`public_url`](crate::config::public_url)). Unset, each is written
    /// out.
    pub public_url: Option<String>,
    /// The name of this process, which every report of the operator surface
    /// and the export of the identity state carry: 1 to 64 ASCII letters,
    /// digits, `.`, `_` and `-`. Unset, it is the `HOSTNAME` the process
    /// runs with, which Kubernetes sets to the pod's name, and `unnamed`
    /// without one ([`Gateway::resolve_replica`]).
    pub replica: Option<String>,
    /// How long the bindings may take to stop their processes once the drain
    /// has ended, such as deleting the PMIR subscription; a process still
    /// running when it elapses is abandoned with a warning. Zero is refused.
    pub bindings_drain_timeout_ms: u64,
}

impl Default for Gateway {
    fn default() -> Self {
        Self {
            public_url: None,
            replica: None,
            bindings_drain_timeout_ms: 5_000,
        }
    }
}

impl Gateway {
    /// Resolves `gateway.public_url` under `base_path`, whose path the URL
    /// must be.
    ///
    /// # Errors
    ///
    /// The errors of [`PublicUrl::resolve`].
    pub fn resolve_public_url(&self, base_path: &BasePath) -> Result<Option<PublicUrl>, Error> {
        self.public_url
            .as_deref()
            .map(|text| PublicUrl::resolve(text, base_path))
            .transpose()
    }

    /// Resolves `gateway.replica`, the name of this process, with the
    /// [`HOST_NAME_ENV`] the process runs with as the name when none is
    /// configured ([`Gateway::replica_or_host`]).
    ///
    /// # Errors
    ///
    /// [`ServerFault::Replica`] for a configured name that is no replica name.
    pub fn resolve_replica(&self) -> Result<Replica, Error> {
        self.replica_or_host(std::env::var(HOST_NAME_ENV).ok().as_deref())
    }

    /// Resolves `gateway.replica`, with `host_name` as the name when none is
    /// configured.
    ///
    /// A host name that is no replica name, or none, names the process
    /// [`UNNAMED`](ferrofed_registry::replica::UNNAMED): the platform's
    /// host name is never a reason to refuse a start.
    ///
    /// # Errors
    ///
    /// [`ServerFault::Replica`] for a configured name that is no replica name.
    pub fn replica_or_host(&self, host_name: Option<&str>) -> Result<Replica, Error> {
        match &self.replica {
            Some(configured) => configured
                .parse()
                .map_err(|source| Error::Server(ServerFault::Replica(source))),
            // NOTE: no specification governs this: our own design; a host name outside the
            // form is legitimately absent as a name, so the process is unnamed.
            None => Ok(host_name
                .and_then(|host| host.parse().ok())
                .unwrap_or_default()),
        }
    }

    /// Resolves `gateway.bindings_drain_timeout_ms`.
    ///
    /// # Errors
    ///
    /// [`Error::Zero`] for a bound of zero.
    pub fn resolve_bindings_drain(&self) -> Result<NonZeroU64, Error> {
        NonZeroU64::new(self.bindings_drain_timeout_ms).ok_or_else(|| Error::Zero {
            key: String::from("gateway.bindings_drain_timeout_ms"),
        })
    }
}
