// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`] with every `_file` secret read.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ferrofed_registry::secret::Secret;
use secrecy::SecretString;
use secrecy::zeroize::Zeroizing;
use url::Url;

use crate::config::error::Error;
use crate::config::{Config, Oidc};

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The directory holding the site bundle.
    pub site_root: PathBuf,
    /// The gateway the console is a client of.
    pub gateway: GatewaySettings,
    /// The server-side sign-in sessions.
    pub session: SessionSettings,
    /// The OpenID Provider operators sign in with, when one is configured.
    pub oidc: Option<OidcSettings>,
}

/// The resolved `[gateway]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySettings {
    /// The gateway's `{base}`, an `http` or `https` URL.
    pub base: Url,
    /// How long one call to the gateway may take.
    pub timeout: Duration,
}

/// The resolved `[session]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    /// Whether the session cookie carries `Secure`.
    pub secure_cookie: bool,
    /// How long a session lives without a request.
    pub idle_timeout: Duration,
    /// How many sessions the console holds at once.
    pub max_sessions: usize,
}

/// The resolved `[oidc]`.
#[derive(Debug, Clone)]
pub struct OidcSettings {
    /// The provider's issuer identifier.
    pub issuer: Url,
    /// The provider's authorization endpoint.
    pub authorization_endpoint: Url,
    /// The console's client identifier.
    pub client_id: String,
    /// The console's client secret, when it is a confidential client.
    pub client_secret: Option<SecretString>,
    /// The console's redirection endpoint.
    pub redirect_uri: Url,
    /// The scopes requested, `openid` among them.
    pub scopes: Vec<String>,
}

impl Config {
    /// Checks every value and reads every `_file` secret.
    ///
    /// # Errors
    /// Returns [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Url`] and [`Error::UrlShape`] for a URL that does not parse
    /// or is not one its key admits, [`Error::Zero`] for a zero timeout or
    /// session bound, [`Error::Missing`] for an absent `[oidc]` key, and the
    /// secret errors of a `_file`.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let base = web_url("gateway.base_url", &self.gateway.base_url)?;
        let timeout =
            positive("gateway.timeout_ms", self.gateway.timeout_ms).map(Duration::from_millis)?;
        let idle_timeout = positive("session.idle_timeout_s", self.session.idle_timeout_s)
            .map(Duration::from_secs)?;
        if self.session.max_sessions == 0 {
            return Err(Error::Zero {
                key: String::from("session.max_sessions"),
            });
        }
        let oidc = self.oidc.as_ref().map(resolve_oidc).transpose()?;
        Ok(Settings {
            listen,
            site_root: self.server.site_root.clone(),
            gateway: GatewaySettings { base, timeout },
            session: SessionSettings {
                secure_cookie: self.session.secure_cookie,
                idle_timeout,
                max_sessions: self.session.max_sessions,
            },
            oidc,
        })
    }
}

/// Resolves `[oidc]`, reading its client secret.
fn resolve_oidc(oidc: &Oidc) -> Result<OidcSettings, Error> {
    let issuer = provider_url("oidc.issuer", &oidc.issuer)?;
    let authorization_endpoint =
        provider_url("oidc.authorization_endpoint", &oidc.authorization_endpoint)?;
    let redirect_uri = web_url("oidc.redirect_uri", &oidc.redirect_uri)?;
    // NOTE: RFC 6749 §3.1.2: the redirection endpoint URI MUST NOT include a
    // fragment component.
    if redirect_uri.fragment().is_some() {
        return Err(Error::UrlShape {
            key: String::from("oidc.redirect_uri"),
            reason: "must not carry a fragment",
        });
    }
    if oidc.client_id.trim().is_empty() {
        return Err(Error::Missing {
            key: String::from("oidc.client_id"),
        });
    }
    // NOTE: OpenID Connect Core 1.0 §3.1.2.1: an OpenID Connect request MUST
    // carry the `openid` scope value.
    if !oidc.scopes.iter().any(|scope| scope == "openid") {
        return Err(Error::Missing {
            key: String::from("oidc.scopes (openid)"),
        });
    }
    let client_secret = secret(
        "oidc.client_secret",
        oidc.client_secret.as_ref(),
        oidc.client_secret_file.as_deref(),
    )?;
    Ok(OidcSettings {
        issuer,
        authorization_endpoint,
        client_id: oidc.client_id.clone(),
        client_secret,
        redirect_uri,
        scopes: oidc.scopes.clone(),
    })
}

/// Returns `value`, refusing zero.
fn positive(key: &str, value: u64) -> Result<u64, Error> {
    if value == 0 {
        Err(Error::Zero {
            key: key.to_owned(),
        })
    } else {
        Ok(value)
    }
}

/// Parses an `http` or `https` URL with a host and no userinfo.
fn web_url(key: &str, text: &str) -> Result<Url, Error> {
    if text.trim().is_empty() {
        return Err(Error::Missing {
            key: key.to_owned(),
        });
    }
    let url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    let shape = |reason| Error::UrlShape {
        key: key.to_owned(),
        reason,
    };
    if !matches!(url.scheme(), "http" | "https") {
        return Err(shape("must be an http or https URL"));
    }
    if url.host().is_none() {
        return Err(shape("must name a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(shape("must not carry userinfo"));
    }
    Ok(url)
}

/// Parses an OpenID Provider URL: `https`, or `http` on a loopback host.
fn provider_url(key: &str, text: &str) -> Result<Url, Error> {
    let url = web_url(key, text)?;
    // NOTE: RFC 6749 §3.1: the authorization server MUST require TLS at its
    // authorization endpoint, so plain `http` is admitted only on loopback.
    let loopback = match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() == "https" || loopback {
        Ok(url)
    } else {
        Err(Error::UrlShape {
            key: key.to_owned(),
            reason: "must be an https URL unless its host is loopback",
        })
    }
}

/// Returns the secret set inline or in the file its `_file` sibling names.
fn secret(
    key: &str,
    inline: Option<&Secret>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(value.to_secret_string())),
        (None, Some(path)) => read_secret(&format!("{key}_file"), path).map(Some),
        (None, None) => Ok(None),
    }
}

/// Reads the secret file `path`, trimmed and refused when empty; the text
/// read is zeroed once the trimmed value is taken.
fn read_secret(key: &str, path: &Path) -> Result<SecretString, Error> {
    let text = std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .map_err(|source| Error::Secret {
            key: key.to_owned(),
            path: path.to_path_buf(),
            source,
        })?;
    let value = text.trim();
    if value.is_empty() {
        return Err(Error::EmptySecret {
            key: key.to_owned(),
            path: path.to_path_buf(),
        });
    }
    Ok(SecretString::from(value))
}
