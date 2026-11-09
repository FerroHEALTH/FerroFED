// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checked settings the console runs on, resolved from the written
//! [`Config`] with every `_file` secret read.

use ferrohealth_config::removed::{Removals, Retired};
use std::net::SocketAddr;
use std::time::Duration;

use ferrohealth_session::config::{OidcSection, SessionSettings};
use ferrohealth_ui::i18n::Locale;
use url::Url;

use crate::config::error::Error;
use crate::config::{Config, Replica};

/// The resolved configuration.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The socket address the console listens on.
    pub listen: SocketAddr,
    /// The locale a page renders in when the request names none the
    /// console's catalog holds.
    pub locale: Locale,
    /// The gateway the console is a client of.
    pub gateway: GatewaySettings,
    /// The server-side sign-in sessions.
    pub session: SessionSettings,
    /// The OpenID Provider operators sign in with, when one is configured.
    pub oidc: Option<OidcSettings>,
    /// The admin listener of every gateway replica the console acts on, in
    /// the order configured; empty when the console offers no admin action.
    pub admin: Vec<ReplicaSettings>,
    /// The console log, the family's `[telemetry]` section.
    pub telemetry: ferrohealth_telemetry::section::TelemetrySection,
}

/// One resolved `[[admin.replicas]]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicaSettings {
    /// The name the console shows and the operator picks the replica by.
    pub name: String,
    /// The root of the replica's admin listener.
    pub url: Url,
}

/// The resolved `[gateway]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySettings {
    /// The gateway's `{base}`, an `http` or `https` URL.
    pub base: Url,
    /// How long one call to the gateway may take.
    pub timeout: Duration,
}

/// The resolved `[oidc]`: the section `ferrohealth-session` checked, and
/// what the console reads from it for its own pages.
#[derive(Debug, Clone)]
pub struct OidcSettings {
    /// The checked section, which builds the console's relying party.
    pub checked: ferrohealth_session::config::OidcSettings,
    /// The console's redirection endpoint, whose origin is the console's.
    redirect_uri: Url,
}

/// `gateway.timeout_ms`, retired in 0.0.19 for `gateway.timeout`; the
/// release after it refuses the key as unknown.
const GATEWAY_TIMEOUT: Retired = Retired::replaced(
    "gateway.timeout_ms",
    "gateway.timeout, a duration such as `30s`",
    "0.0.19",
);

impl OidcSettings {
    /// The console's own origin, `scheme://host:port`: the origin of its
    /// redirection endpoint, which an unsafe request must come from.
    #[must_use]
    pub fn origin(&self) -> String {
        self.redirect_uri.origin().ascii_serialization()
    }

    /// The console's redirection endpoint.
    #[must_use]
    pub const fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }
}

impl Config {
    /// Checks every value and reads every `_file` secret.
    ///
    /// # Errors
    /// Returns [`Error::Address`] for a listen address that does not parse,
    /// [`Error::Locale`] for a `server.locale` that is no language tag,
    /// [`Error::Url`] and [`Error::UrlShape`] for a URL that does not parse
    /// or is not one its key admits, [`Error::Zero`] for a zero timeout or
    /// session bound, [`Error::Missing`] for an absent `[oidc]` key,
    /// [`Error::Scope`] for `offline_access` listed in `oidc.scopes`,
    /// [`Error::InsecureCookie`] for `secure_cookie = false` on a console
    /// whose `redirect_uri` is not `http` on loopback, and the secret errors
    /// of a `_file`, and [`Error::Retired`] for a key a release retired.
    pub fn resolve(&self) -> Result<Settings, Error> {
        let mut removals = Removals::new();
        removals.check(self.gateway.timeout_ms, &GATEWAY_TIMEOUT);
        removals.finish()?;
        let listen = self
            .server
            .listen
            .parse()
            .map_err(|source| Error::Address {
                key: String::from("server.listen"),
                source,
            })?;
        let locale =
            crate::locale::configured(&self.server.locale).map_err(|source| Error::Locale {
                key: String::from("server.locale"),
                source,
            })?;
        let base = web_url("gateway.base_url", &self.gateway.base_url)?;
        let timeout = positive("gateway.timeout", self.gateway.timeout.as_millis())
            .map(Duration::from_millis)?;
        let oidc = self.oidc.as_ref().map(resolve_oidc).transpose()?;
        let session = match &oidc {
            Some(oidc) => self.session.check(&oidc.checked)?,
            None => self.session.check_with(false)?,
        };
        let admin = resolve_admin(&self.admin.replicas)?;
        // NOTE: RFC 6265bis §4.1.2.5: a cookie without `Secure` travels over plain
        // HTTP, so it is admitted only where the console itself is HTTP on loopback.
        if !session.secure_cookie()
            && oidc.as_ref().is_some_and(|oidc| {
                !(oidc.redirect_uri.scheme() == "http" && is_loopback(&oidc.redirect_uri))
            })
        {
            return Err(Error::InsecureCookie {
                key: String::from("session.secure_cookie"),
            });
        }
        Ok(Settings {
            listen,
            locale,
            gateway: GatewaySettings { base, timeout },
            session,
            oidc,
            admin,
            telemetry: self.telemetry.clone(),
        })
    }
}

/// Resolves `[[admin.replicas]]`, each name checked and given once.
fn resolve_admin(replicas: &[Replica]) -> Result<Vec<ReplicaSettings>, Error> {
    let mut resolved: Vec<ReplicaSettings> = Vec::with_capacity(replicas.len());
    for replica in replicas {
        let key = "admin.replicas.name";
        let name = replica.name.trim();
        if name.is_empty() {
            return Err(Error::Missing {
                key: key.to_owned(),
            });
        }
        // NOTE: no specification governs this: our own design; a name the console
        // shows and posts back is short plain text, so it needs no escaping anywhere.
        let plain = name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !plain {
            return Err(Error::ReplicaName {
                key: key.to_owned(),
                reason: "must be letters, digits, '.', '_' and '-', at most 64 of them",
            });
        }
        if resolved.iter().any(|known| known.name == name) {
            return Err(Error::ReplicaName {
                key: key.to_owned(),
                reason: "must name each replica once",
            });
        }
        let url = web_url("admin.replicas.admin_url", &replica.admin_url)?;
        resolved.push(ReplicaSettings {
            name: name.to_owned(),
            url,
        });
    }
    Ok(resolved)
}

/// Resolves `[oidc]`: the section's check, which holds the provider, the
/// redirection endpoint and the scopes to their rules and reads the client
/// secret, and the console's own rule for a post-logout redirect.
fn resolve_oidc(oidc: &OidcSection) -> Result<OidcSettings, Error> {
    let checked = oidc.check()?;
    let redirect_uri = Url::parse(&oidc.redirect_uri).map_err(|source| Error::Url {
        key: String::from("oidc.redirect_uri"),
        source,
    })?;
    let endpoints_given = oidc.authorization_endpoint.is_some()
        && oidc.token_endpoint.is_some()
        && oidc.jwks_uri.is_some();
    // NOTE: OpenID Connect RP-Initiated Logout 1.0 §3: the post-logout redirect is sent to
    // the end-session endpoint, so one the written endpoints lack is refused before discovery.
    if oidc.post_logout_redirect_uri.is_some()
        && endpoints_given
        && oidc.end_session_endpoint.is_none()
    {
        return Err(Error::Missing {
            key: String::from("oidc.end_session_endpoint (for post_logout_redirect_uri)"),
        });
    }
    Ok(OidcSettings {
        checked,
        redirect_uri,
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

/// Whether `url` names a loopback host: `localhost`, `127.0.0.0/8` or `::1`.
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}
