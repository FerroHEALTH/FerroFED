// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's configuration: a TOML file, `FERROFED_VIEWER__` environment
//! overrides over it, and every secret reachable through a `<key>_file`
//! sibling read at start.
//!
//! [`Config`] is the tree as written, every default in its struct's
//! `Default` impl; [`Config::load`](crate::config::Config::load) reads it and
//! [`Config::resolve`](crate::config::Config::resolve) turns it into the
//! checked [`settings::Settings`] the server runs on. An override names its
//! key by path, `FERROFED_VIEWER__GATEWAY__BASE_URL` for `[gateway]
//! base_url`. No specification governs the configuration: our own design.
//!
//! ```toml
//! [server]
//! listen = "0.0.0.0:3000"
//! site_root = "/site"
//!
//! [gateway]
//! base_url = "https://gateway.example.org/"
//!
//! [oidc]
//! issuer = "https://idp.example.org/realms/ferrofed"
//! authorization_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth"
//! client_id = "ferrofed-viewer"
//! client_secret_file = "/run/secrets/viewer-client-secret"
//! redirect_uri = "https://console.example.org/auth/callback"
//! ```

pub mod error;
pub mod load;
pub mod settings;

use std::path::PathBuf;

use ferrofed_registry::secret::Secret;
use serde::Deserialize;

/// The environment variable that names the configuration file when
/// `--config` does not.
pub const CONFIG_PATH_ENV: &str = "FERROFED_VIEWER_CONFIG";

/// The prefix of every environment override.
pub const ENV_PREFIX: &str = "FERROFED_VIEWER__";

/// The configuration tree as written.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[server]`: where the console listens and where its site bundle is.
    pub server: Server,
    /// `[gateway]`: the FerroFED gateway the console is a client of.
    pub gateway: Gateway,
    /// `[session]`: the server-side sign-in sessions.
    pub session: Session,
    /// `[oidc]`: the OpenID Provider operators sign in with; without it the
    /// console offers no sign-in.
    pub oidc: Option<Oidc>,
}

/// `[server]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address the console listens on.
    pub listen: String,
    /// The directory holding the site bundle cargo-leptos builds: the
    /// WebAssembly, its JavaScript glue and the stylesheet under `pkg/`.
    pub site_root: PathBuf,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:3000"),
            site_root: PathBuf::from("target/site"),
        }
    }
}

/// `[gateway]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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

/// `[session]`: the pending sign-ins and the signed-in sessions, each a pool
/// of its own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Session {
    /// Whether the cookies carry `Secure`, so a browser sends them over HTTPS
    /// only; off only for a console on loopback.
    pub secure_cookie: bool,
    /// How long a sign-in begun at `GET /login` waits for the provider's
    /// redirect back, in seconds.
    pub sign_in_timeout_s: u64,
    /// How many pending sign-ins the console holds at once; a new one drops
    /// the oldest.
    pub max_sign_ins: usize,
    /// How long a signed-in session lives without a request, in seconds.
    pub idle_timeout_s: u64,
    /// How long a signed-in session lives at most, in seconds.
    pub absolute_timeout_s: u64,
    /// How many signed-in sessions the console holds at once.
    pub max_sessions: usize,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            secure_cookie: true,
            sign_in_timeout_s: 300,
            max_sign_ins: 1000,
            idle_timeout_s: 1800,
            absolute_timeout_s: 43_200,
            max_sessions: 10_000,
        }
    }
}

/// `[oidc]`: an OAuth 2.0 authorization code client with PKCE at an OpenID
/// Provider.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Oidc {
    /// The provider's issuer identifier.
    pub issuer: String,
    /// The provider's authorization endpoint (RFC 6749 §3.1).
    pub authorization_endpoint: String,
    /// The console's client identifier at the provider (RFC 6749 §2.2).
    pub client_id: String,
    /// The console's client secret, inline.
    pub client_secret: Option<Secret>,
    /// A file holding the console's client secret.
    pub client_secret_file: Option<PathBuf>,
    /// The redirection endpoint the provider sends the operator back to
    /// (RFC 6749 §3.1.2), the console's `/auth/callback`.
    pub redirect_uri: String,
    /// The scopes requested, `openid` among them.
    pub scopes: Vec<String>,
}

impl Default for Oidc {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            authorization_endpoint: String::new(),
            client_id: String::new(),
            client_secret: None,
            client_secret_file: None,
            redirect_uri: String::new(),
            scopes: vec![String::from("openid")],
        }
    }
}
