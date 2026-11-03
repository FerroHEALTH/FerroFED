// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration, read by `ferrohealth-config`.
//!
//! The tree's defaults, a TOML file, `FERROFED_CONSOLE__` environment
//! overrides and the `--set` pairs over it, and every secret reachable
//! through a `<key>_file` sibling read at start through `ferrohealth-secret`.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`](crate::config::Config::load) reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROFED_CONSOLE__GATEWAY__BASE_URL` for `[gateway]
//! base_url`; `RUST_LOG` is the alias of `telemetry.filter`. No
//! specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//!
//! [gateway]
//! base_url = "https://gateway.example.org/"
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferrofed"
//! authorization_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth"
//! token_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/token"
//! jwks_uri = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs"
//! client_id = "ferrofed-console"
//! client_secret_file = "/run/secrets/console-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! end_session_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/logout"
//! post_logout_redirect_uri = "https://console.example.org/"
//! offline_access = true
//!
//! [[admin.replicas]]
//! name = "gateway-0"
//! admin_url = "http://ferrofed-0.ferrofed-admin.internal:9464/"
//! ```

pub mod error;
pub mod load;
pub mod settings;

use ferrohealth_config::loader::Loader;
use ferrohealth_session::config::{OidcSection, SessionSection};
use ferrohealth_telemetry::section::TelemetrySection;
use serde::{Deserialize, Serialize};

/// The environment namespace: `FERROFED_CONSOLE__SERVER__LISTEN` sets
/// `[server] listen`, and `FERROFED_CONSOLE_CONFIG` names the file.
pub const PREFIX: &str = "FERROFED_CONSOLE";

/// The variable that names the `tracing` filter, the alias of
/// `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The loader of the console's tree.
///
/// # Errors
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ferrohealth_config::error::ConfigError> {
    Ok(Loader::new(PREFIX)?.alias(FILTER_ENV, "telemetry.filter"))
}

/// The configuration tree as written.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and the locale it falls back to.
    pub server: Server,
    /// `[gateway]`: the FerroFED gateway the console is a client of.
    pub gateway: Gateway,
    /// `[session]`: the store and the lifetimes of the sign-in sessions,
    /// `ferrohealth-session`'s section.
    pub session: SessionSection,
    /// `[oidc]`: the OpenID Provider operators sign in with,
    /// `ferrohealth-session`'s section; without it the console offers no
    /// sign-in.
    pub oidc: Option<OidcSection>,
    /// `[admin]`: the gateway replicas whose admin listener the console
    /// reaches for the operator's admin actions; without one the console
    /// offers none.
    pub admin: Admin,
    /// `[telemetry]`: the console log, the family's section; `RUST_LOG` sets
    /// its filter too.
    pub telemetry: TelemetrySection,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The BCP 47 tag of the locale a page renders in when the request's
    /// `Accept-Language` names no language the console's catalog holds.
    pub locale: String,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            locale: String::from("en"),
        }
    }
}

/// `[gateway]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gateway {
    /// The gateway's `{base}`: its self-description is `OPTIONS {base}/` and
    /// its ITS-REST surface is under `{base}/v1`.
    pub base_url: String,
    /// How long one call to the gateway may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Gateway {
    fn default() -> Self {
        Self {
            base_url: String::from("http://127.0.0.1:8080/"),
            timeout_ms: 30_000,
        }
    }
}

/// `[admin]`: the admin listeners of the gateway's replicas, which the
/// console's server calls with the operator's own access token for the
/// collision clear and the stored-query drift repair.
///
/// Each admin listener must be reachable from the console's server on an
/// internal network, never a public one, so the gateway's `[metrics] listen`
/// binds a non-loopback internal address.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Admin {
    /// Every replica, in the order the console names them; the drift
    /// repair, which acts on the store the replicas share, goes to the
    /// first.
    pub replicas: Vec<Replica>,
}

/// One gateway replica's admin listener.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Replica {
    /// The name the console shows for the replica and the operator picks it
    /// by: letters, digits, `.`, `_` and `-`, at most 64 of them.
    pub name: String,
    /// The root of the replica's admin listener, an `http` or `https` URL.
    pub admin_url: String,
}
