// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The transport every outbound connection the configuration names must use,
//! by what it carries.
//!
//! Four policies hold, each written once here and applied by its callers
//! (`crate::onward` holds a node's client identity to
//! [`client_certificate`]); the mutual-TLS grant rules that are no transport
//! policy, one binding per token and the alias hosts, sit in `config::grant`:
//!
//! - **Protected payload** ([`protected_payload`]): a URL a credential or a
//!   patient identifier is sent to must be `https` outside
//!   `profile = "development"`. Under that profile it may be `http`, and each
//!   such site is reported so the banner, the log and `config check` name it.
//! - **Encrypted connection** ([`encrypted_connection`]): a database
//!   connection that carries a password must require TLS outside the
//!   development profile, reported under it as a protected payload is.
//! - **Trust anchor** ([`trust_anchor`]): a URL the gateway verifies its
//!   callers against, a key set or a token introspection endpoint, must be
//!   `https`, or `http` to a loopback host, under every profile, since a
//!   verifier reached in the clear lets a network attacker forge callers.
//! - **Client certificate** ([`client_certificate`]): a URL the gateway
//!   presents its TLS client certificate to, a node with a client identity
//!   and the token endpoint or issuer of its grant that uses mutual TLS
//!   (RFC 8705), must be `https` under every profile, since a client
//!   certificate needs the TLS handshake it is presented in.
//!
//! The protected-payload sites of the core are a registry endpoint with a
//! `[credentials."<id>"]` section, the token endpoint of that section's OAuth
//! 2.0 grant, the issuer of its FAPI 2.0 grant (sent the client assertion and
//! the callers' tokens), the site of a grant a binding adds, and
//! `metrics.otlp_endpoint` and `telemetry.otlp_endpoint` when either carries
//! a user name or a password. Each binding holds its own sites
//! ([`Binding::sites`](crate::binding::Binding::sites)): the IHE binding every
//! PIX Manager, the PDQm Supplier of `[pdqm]`, every XCPD responding gateway,
//! the Patient Identity Registry of `[pmir]` and its callback URL, the care
//! services directory of `[registry.mcsd]` when it has credentials, the
//! audit repository of `[audit]`, and the ATNA Audit Record Repository the
//! XCPD audit messages go to, which must be `tls://` ([`encrypted_syslog`]);
//! the Dutch binding the NVI Localization Service of `[nl_gf.nvi]`, Mitz of
//! `[nl_gf.mitz]` and the authorization server of a Nuts grant. Any other URL
//! may stay `http`. The encrypted-connection site is the stored-query store's
//! PostgreSQL connection string, whose `sslmode` must be `require` when it
//! carries a password and reaches a host over the network.
//!
//! The specification assumes transport security and binds it through the
//! security profiles (§2.2, §13, Annex B); no specification governs these
//! policies: our own design.

use std::fmt;

use ferrofed_identity::dev::Profile;
use ferrofed_registry::snapshot::RegistrySnapshot;
use url::Url;

use crate::config::settings::{Scheme, Settings};
#[cfg(feature = "postgres")]
use crate::config::stored_queries::Store;

/// Where a protected payload is sent: the key of the URL and what travels to
/// it, by key, never a value, with the encryption it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedSite {
    /// The key of the URL, such as `pixm.manager[0].url`.
    pub url_key: String,
    /// What travels to the URL, by key, such as
    /// `pixm.manager[0].credentials and patient identifiers`.
    pub payload: String,
    /// The encryption the site needs outside the development profile.
    pub requires: Encryption,
}

/// The encryption a protected site needs outside the development profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Encryption {
    /// An `https` URL.
    Https,
    /// A database connection that requires TLS (`sslmode=require`).
    Tls,
    /// Syslog over TLS (RFC 5425), a `tls://` address.
    SyslogTls,
}

impl fmt::Display for Encryption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Https => "an https URL",
            Self::Tls => "a connection that requires TLS (sslmode=require)",
            Self::SyslogTls => "a tls:// syslog address (RFC 5425)",
        })
    }
}

/// A credential or a patient identifier configured to travel unencrypted,
/// outside the development profile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{} is not {}, and {} would travel over it in cleartext: outside profile = \"development\" a credential or a patient identifier is sent only encrypted",
    site.url_key,
    site.requires,
    site.payload
)]
pub struct CleartextError {
    /// The site that would send its payload in cleartext.
    pub site: ProtectedSite,
}

/// A trust anchor configured over plain `http` to a host that is not
/// loopback.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{key} is plain http to a host that is not loopback: the gateway verifies its callers against it, so it is https, or http to a loopback host, under every profile"
)]
pub struct TrustAnchorError {
    /// The key that carries the URL.
    pub key: String,
}

/// Holds one site to the protected-payload policy.
///
/// It returns `Ok(None)` when `url` is `https`, and `Ok(Some(site))` when it
/// is not and `profile` is development, so the caller reports that the
/// payload travels unencrypted.
/// A URL that does not parse cannot be shown to be `https`, so it counts as
/// unprotected. The URL is read for its scheme and never rendered.
///
/// # Errors
///
/// Returns [`CleartextError`] naming `site` when `url` is not `https` and
/// `profile` is not development.
pub fn protected_payload(
    profile: Profile,
    url: &str,
    site: ProtectedSite,
) -> Result<Option<ProtectedSite>, CleartextError> {
    let protected = Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https");
    admit(profile, protected, site)
}

/// Holds one database connection to the encrypted-connection policy:
/// `requires_tls` says whether the connection requires TLS, or sends no
/// password over a network.
///
/// It answers as [`protected_payload`] does.
///
/// # Errors
///
/// Returns [`CleartextError`] naming `site` when the connection does not
/// require TLS and `profile` is not development.
pub fn encrypted_connection(
    profile: Profile,
    requires_tls: bool,
    site: ProtectedSite,
) -> Result<Option<ProtectedSite>, CleartextError> {
    admit(profile, requires_tls, site)
}

/// Holds a syslog address to the protected-payload policy: `tls://` (RFC
/// 5425), or anything else under development alone.
///
/// It answers as [`protected_payload`] does.
///
/// # Errors
///
/// Returns [`CleartextError`] naming `site` when `url` is not `tls://` and
/// `profile` is not development.
pub fn encrypted_syslog(
    profile: Profile,
    url: &Url,
    site: ProtectedSite,
) -> Result<Option<ProtectedSite>, CleartextError> {
    admit(profile, url.scheme() == "tls", site)
}

/// The answer for `site`, `protected` or not, under `profile`.
fn admit(
    profile: Profile,
    protected: bool,
    site: ProtectedSite,
) -> Result<Option<ProtectedSite>, CleartextError> {
    if protected {
        return Ok(None);
    }
    match profile {
        Profile::Development => Ok(Some(site)),
        Profile::Production => Err(CleartextError { site }),
    }
}

/// Whether `url` may be presented the gateway's TLS client certificate.
///
/// Only an `https` URL may, under every profile (RFC 8705 §2, §3). The
/// caller names the site in its own refusal; the URL is read for its scheme
/// and never rendered.
#[must_use]
pub fn client_certificate(url: &str) -> bool {
    Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https")
}

/// Holds the URL at `key` to the trust-anchor policy: `https`, or `http` to a
/// loopback host, under every profile.
///
/// # Errors
///
/// Returns [`TrustAnchorError`] naming `key` for any other URL.
pub fn trust_anchor(key: &str, url: &Url) -> Result<(), TrustAnchorError> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(url) => Ok(()),
        _ => Err(TrustAnchorError {
            key: key.to_owned(),
        }),
    }
}

/// Whether `url` names a loopback host: an IPv4 or IPv6 loopback address,
/// or `localhost`.
#[must_use]
pub fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    }
}

/// Holds one site to the stricter policy of a run that carries a caller's
/// bearer token and writes synthetic data: `https`, or `http` to a loopback
/// host under the development profile alone.
///
/// It returns `Ok(None)` for `https`, and `Ok(Some(site))` for loopback
/// `http` under the development profile, so the caller reports that the
/// payload travels unencrypted. The URL is read for its scheme and host and
/// never rendered.
///
/// # Errors
///
/// Returns [`CleartextError`] naming `site` for plain `http` to a host that
/// is not loopback under every profile, for any `http` outside the
/// development profile, and for any other scheme.
pub fn loopback_payload(
    profile: Profile,
    url: &Url,
    site: ProtectedSite,
) -> Result<Option<ProtectedSite>, CleartextError> {
    match url.scheme() {
        "https" => Ok(None),
        "http" if is_loopback(url) && profile == Profile::Development => Ok(Some(site)),
        _ => Err(CleartextError { site }),
    }
}
/// The sites each endpoint's onward credentials send to: the endpoint's own
/// URL in `registry`, an OAuth 2.0 grant's token endpoint, a Nuts grant's
/// authorization server and a FAPI 2.0 grant's issuer, in key order.
fn credential_sites(
    settings: &Settings,
    registry: Option<&RegistrySnapshot>,
) -> Vec<(String, ProtectedSite)> {
    let site = |url_key: String, payload: String| ProtectedSite {
        url_key,
        payload,
        requires: Encryption::Https,
    };
    let mut sites = Vec::new();
    for (endpoint, scheme) in &settings.credentials {
        let section = format!("credentials.{endpoint}");
        // NOTE: no specification governs this: our own design; an endpoint the
        // registry lacks is legitimately absent here, and the client build refuses it.
        if let Some(declared) = registry.and_then(|registry| registry.endpoint(endpoint)) {
            sites.push((
                declared.url().as_str().to_owned(),
                site(
                    format!("the url of endpoint {endpoint} in registry.document"),
                    section.clone(),
                ),
            ));
        }
        if let Scheme::OAuth2(grant) = scheme {
            sites.push((
                grant.token_endpoint().as_str().to_owned(),
                site(
                    format!("{section}.oauth2.token_endpoint"),
                    format!("{section}.oauth2"),
                ),
            ));
        }
        if let Scheme::Binding(grant) = scheme {
            sites.push(grant.site(&section));
        }
        // NOTE: FAPI 2.0 Security Profile §5.2.1, every endpoint is TLS-protected; the
        // token endpoint the metadata names is held to the issuer's origin.
        if let Scheme::Fapi2(grant) = scheme {
            sites.push((
                grant.issuer().as_str().to_owned(),
                site(
                    format!("{section}.fapi2.issuer"),
                    format!("{section}.fapi2 client assertion and caller tokens"),
                ),
            ));
        }
    }
    sites
}

/// Holds every protected payload `settings` send to [`protected_payload`].
///
/// The endpoint URLs are those `registry` declares. The sites that travel in
/// cleartext under the development profile are returned in key order.
/// Without `registry`, the endpoint URLs are not known and only the other
/// sites are held. An endpoint `[credentials]` names that `registry` does not
/// declare has no URL to hold; building the node clients refuses it.
///
/// # Errors
///
/// Returns the [`CleartextError`] of the first site, in key order, that
/// [`protected_payload`] refuses.
pub fn check(
    settings: &Settings,
    registry: Option<&RegistrySnapshot>,
) -> Result<Vec<ProtectedSite>, CleartextError> {
    let profile = settings.profile;
    let mut cleartext = Vec::new();
    for (url, site) in credential_sites(settings, registry) {
        cleartext.extend(protected_payload(profile, &url, site)?);
    }
    for binding in crate::binding::compiled() {
        cleartext.extend(binding.sites(settings)?);
    }
    let collectors = [
        ("metrics.otlp_endpoint", &settings.metrics.otlp_endpoint),
        ("telemetry.otlp_endpoint", &settings.telemetry.otlp_endpoint),
    ];
    for (key, endpoint) in collectors {
        let Some(endpoint) = endpoint else {
            continue;
        };
        let carries = Url::parse(endpoint.expose())
            .is_ok_and(|parsed| !parsed.username().is_empty() || parsed.password().is_some());
        if carries {
            let site = ProtectedSite {
                url_key: key.to_owned(),
                payload: format!("the userinfo of {key}"),
                requires: Encryption::Https,
            };
            cleartext.extend(protected_payload(profile, endpoint.expose(), site)?);
        }
    }
    #[cfg(feature = "postgres")]
    if let Some(Store::Postgres(url)) = &settings.stored_queries {
        let site = ProtectedSite {
            url_key: String::from("stored_queries.url"),
            payload: String::from("the password in stored_queries.url"),
            requires: Encryption::Tls,
        };
        let requires_tls = !crate::stored::postgres::exposes_password(url);
        cleartext.extend(encrypted_connection(profile, requires_tls, site)?);
    }
    Ok(cleartext)
}

/// The site of the identity service configured at `key`, such as
/// `pixm.manager[0]`: its `url`, sent the patient identifiers it is asked
/// for, with the credential `credential` names when one is configured.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub(crate) fn identity_site(key: &str, credential: Option<&str>) -> ProtectedSite {
    let payload = match credential {
        Some(credential) => format!("{credential} and patient identifiers"),
        None => format!("the patient identifiers asked of {key}"),
    };
    ProtectedSite {
        url_key: format!("{key}.url"),
        payload,
        requires: Encryption::Https,
    }
}

/// Logs one warning per site in `cleartext`, by key, never a value.
pub fn warn(cleartext: &[ProtectedSite]) {
    for site in cleartext {
        tracing::warn!(
            url = site.url_key,
            payload = site.payload,
            requires = %site.requires,
            "a credential or a patient identifier travels unencrypted, which only the development profile allows"
        );
    }
}

/// Writes one warning per site in `cleartext` to stderr, by key, never a
/// value, for a command that runs with no log subscriber.
#[expect(
    clippy::print_stderr,
    reason = "`config check` and `admission check` warn the operator who ran them"
)]
pub fn print_warnings(cleartext: &[ProtectedSite]) {
    for site in cleartext {
        eprintln!(
            "ferrofed: warning: {} travels unencrypted to {}, which is not {}; only profile = \"development\" allows that",
            site.payload, site.url_key, site.requires
        );
    }
}

/// Holds every protected payload to the policy and warns on stderr.
///
/// It runs [`check`], then [`print_warnings`] for each site that travels in
/// cleartext, for a command that runs with no log subscriber.
///
/// # Errors
///
/// Returns the [`CleartextError`] [`check`] returns.
pub fn check_and_print(
    settings: &Settings,
    registry: Option<&RegistrySnapshot>,
) -> Result<(), CleartextError> {
    print_warnings(&check(settings, registry)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CleartextError, Encryption, ProtectedSite, TrustAnchorError, encrypted_connection,
        loopback_payload, protected_payload, trust_anchor,
    };
    use ferrofed_identity::dev::Profile;
    use url::Url;

    fn site() -> ProtectedSite {
        ProtectedSite {
            url_key: String::from("xcpd.gateway[0].url"),
            payload: String::from("xcpd.assertion and patient identifiers"),
            requires: Encryption::Https,
        }
    }

    #[test]
    fn a_run_admits_https_and_loopback_http_under_development_alone() {
        let url = |text: &str| Url::parse(text).expect("a URL");
        for profile in [Profile::Development, Profile::Production] {
            assert_eq!(
                Ok(None),
                loopback_payload(profile, &url("https://gw.example.org/"), site()),
                "{profile:?}: https"
            );
        }
        for loopback in [
            "http://127.0.0.1:8080/",
            "http://[::1]/",
            "http://localhost/",
        ] {
            assert_eq!(
                Ok(Some(site())),
                loopback_payload(Profile::Development, &url(loopback), site()),
                "{loopback} under development, reported"
            );
            assert!(
                loopback_payload(Profile::Production, &url(loopback), site()).is_err(),
                "{loopback} outside development"
            );
        }
        for refused in [
            "http://gw.example.org/",
            "http://10.0.0.7/",
            "ftp://127.0.0.1/",
        ] {
            assert!(
                loopback_payload(Profile::Development, &url(refused), site()).is_err(),
                "{refused} carries a token in cleartext beyond the host"
            );
        }
    }

    #[test]
    fn a_connection_without_tls_is_refused_outside_development_and_named_by_its_need() {
        let site = ProtectedSite {
            url_key: String::from("stored_queries.url"),
            payload: String::from("the password in stored_queries.url"),
            requires: Encryption::Tls,
        };
        assert_eq!(
            Ok(None),
            encrypted_connection(Profile::Production, true, site.clone())
        );
        assert_eq!(
            Ok(Some(site.clone())),
            encrypted_connection(Profile::Development, false, site.clone())
        );
        let refused = encrypted_connection(Profile::Production, false, site)
            .expect_err("a password without TLS is refused");
        let text = refused.to_string();
        assert!(
            text.contains(
                "stored_queries.url is not a connection that requires TLS (sslmode=require)"
            ),
            "{text}"
        );
    }

    #[test]
    fn https_passes_under_every_profile() {
        for profile in [Profile::Production, Profile::Development] {
            assert_eq!(
                Ok(None),
                protected_payload(profile, "https://pix.example.org/fhir", site())
            );
        }
    }

    #[test]
    fn http_is_refused_outside_development_and_reported_under_it() {
        assert_eq!(
            Err(CleartextError { site: site() }),
            protected_payload(Profile::Production, "http://pix.example.org/fhir", site())
        );
        assert_eq!(
            Ok(Some(site())),
            protected_payload(Profile::Development, "http://pix.example.org/fhir", site())
        );
    }

    #[test]
    fn a_url_that_does_not_parse_counts_as_unprotected() {
        assert_eq!(
            Err(CleartextError { site: site() }),
            protected_payload(Profile::Production, "not a url", site())
        );
    }

    #[test]
    fn the_refusal_names_both_keys_and_never_the_url() {
        let refused = protected_payload(
            Profile::Production,
            "http://user:synthetic-secret@pix.example.org/fhir",
            site(),
        )
        .expect_err("plain http is refused");
        let text = refused.to_string();
        assert!(text.contains("xcpd.gateway[0].url"), "{text}");
        assert!(text.contains("xcpd.assertion"), "{text}");
        assert!(!text.contains("synthetic-secret"), "{text}");
        assert!(!text.contains("pix.example.org"), "{text}");
    }

    #[test]
    fn a_trust_anchor_is_https_or_loopback_http() {
        for good in [
            "https://issuer.example.test/jwks",
            "http://127.0.0.1:8443/jwks",
            "http://[::1]:8443/jwks",
            "http://localhost/jwks",
        ] {
            let url = Url::parse(good).expect("a test URL parses");
            assert_eq!(
                Ok(()),
                trust_anchor("auth.issuer[0].jwks_uri", &url),
                "{good}"
            );
        }
        let url = Url::parse("http://issuer.example.test/jwks").expect("a test URL parses");
        assert_eq!(
            Err(TrustAnchorError {
                key: String::from("auth.issuer[0].jwks_uri")
            }),
            trust_anchor("auth.issuer[0].jwks_uri", &url)
        );
    }
}
