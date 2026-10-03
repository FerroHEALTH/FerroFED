// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The transport every configured credential travels over: outside
//! `profile = "development"`, a URL a credential is sent to must be `https`.
//!
//! The sites are a registry endpoint with a `[credentials."<id>"]` section, the
//! token endpoint of that section's OAuth 2.0 grant, a PIX Manager with
//! credentials, an XCPD responding gateway when an XUA assertion is
//! configured, and `metrics.otlp_endpoint` when it carries a user name or a
//! password. A URL no credential is sent to stays allowed over `http`. The
//! stored-query PostgreSQL connection string is no site: it is never an `http`
//! URL, and whether libpq encrypts it is its own `sslmode`.
//!
//! The specification assumes transport security and binds it through the
//! security profiles (§2.2, §13, Annex B); no specification governs this check:
//! our own design.

use ferrofed_identity::dev::Profile;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::config::settings::{Scheme, Settings};

/// Where one configured credential is sent: the key of the URL and the key of
/// the credential, never a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialSite {
    /// The key of the URL the credential is sent to, such as
    /// `pixm.manager[0].url`.
    pub url_key: String,
    /// The key of the credential, such as `pixm.manager[0].credentials`.
    pub credential: String,
}

/// A credential configured to travel over a URL that is not `https`, outside
/// the development profile.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{} is not an https URL, and {} would travel over it in cleartext: outside profile = \"development\" a credential is sent only over https",
    site.url_key,
    site.credential
)]
pub struct CleartextError {
    /// The site that would send the credential in cleartext.
    pub site: CredentialSite,
}

/// Holds one credential site to the rule: `Ok(None)` when `url` is `https`,
/// `Ok(Some(site))` when it is not and `profile` is development, so the caller
/// warns that the credential travels unencrypted.
///
/// A URL that does not parse cannot be shown to be `https`, so it counts as
/// unprotected. The URL is read for its scheme and never rendered.
///
/// # Errors
///
/// Returns [`CleartextError`] naming `site` when `url` is not `https` and
/// `profile` is not development.
pub fn guard(
    profile: Profile,
    url: &str,
    site: CredentialSite,
) -> Result<Option<CredentialSite>, CleartextError> {
    let protected = url::Url::parse(url).is_ok_and(|parsed| parsed.scheme() == "https");
    if protected {
        return Ok(None);
    }
    match profile {
        Profile::Development => Ok(Some(site)),
        Profile::Production => Err(CleartextError { site }),
    }
}

/// Holds every credential `settings` send to the rule of [`guard`].
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
/// [`guard`] refuses.
pub fn check(
    settings: &Settings,
    registry: Option<&RegistrySnapshot>,
) -> Result<Vec<CredentialSite>, CleartextError> {
    let profile = settings.profile;
    let mut cleartext = Vec::new();
    let mut hold = |url: &str, url_key: String, credential: String| {
        let site = CredentialSite {
            url_key,
            credential,
        };
        guard(profile, url, site).map(|exposed| cleartext.extend(exposed))
    };
    for (endpoint, scheme) in &settings.credentials {
        let section = format!("credentials.{endpoint}");
        // NOTE: no specification governs this: our own design; an endpoint the
        // registry lacks is legitimately absent here, and the client build refuses it.
        if let Some(declared) = registry.and_then(|registry| registry.endpoint(endpoint)) {
            hold(
                declared.url().as_str(),
                format!("the url of endpoint {endpoint} in registry.document"),
                section.clone(),
            )?;
        }
        if let Scheme::OAuth2(grant) = scheme {
            hold(
                grant.token_endpoint().as_str(),
                format!("{section}.oauth2.token_endpoint"),
                format!("{section}.oauth2"),
            )?;
        }
    }
    for (index, manager) in settings
        .pixm
        .iter()
        .flat_map(|pixm| pixm.managers.iter().enumerate())
    {
        if manager.credentials.is_some() {
            let key = format!("pixm.manager[{index}]");
            hold(
                manager.url.expose(),
                format!("{key}.url"),
                format!("{key}.credentials"),
            )?;
        }
    }
    if let Some(xcpd) = settings
        .xcpd
        .as_ref()
        .filter(|xcpd| xcpd.assertion.is_some())
    {
        for (index, gateway) in xcpd.gateways.iter().enumerate() {
            hold(
                gateway.url.expose(),
                format!("xcpd.gateway[{index}].url"),
                String::from(xcpd.assertion_key),
            )?;
        }
    }
    if let Some(endpoint) = &settings.metrics.otlp_endpoint {
        let carries = url::Url::parse(endpoint.expose())
            .is_ok_and(|parsed| !parsed.username().is_empty() || parsed.password().is_some());
        if carries {
            hold(
                endpoint.expose(),
                String::from("metrics.otlp_endpoint"),
                String::from("the userinfo of metrics.otlp_endpoint"),
            )?;
        }
    }
    Ok(cleartext)
}

/// Logs one warning per credential site in `cleartext`, by key, never a
/// value.
pub fn warn(cleartext: &[CredentialSite]) {
    for site in cleartext {
        tracing::warn!(
            url = site.url_key,
            credential = site.credential,
            "a credential travels unencrypted over plain http, which only the development profile allows"
        );
    }
}

/// Writes one warning per credential site in `cleartext` to stderr, by key,
/// never a value, for a command that runs with no log subscriber.
#[expect(
    clippy::print_stderr,
    reason = "`config check` and `admission check` warn the operator who ran them"
)]
pub fn print_warnings(cleartext: &[CredentialSite]) {
    for site in cleartext {
        eprintln!(
            "ferrofed: warning: {} travels unencrypted to {}, which is not https; only profile = \"development\" allows that",
            site.credential, site.url_key
        );
    }
}

/// Holds every credential to the rule, as [`check`] does, and writes a
/// warning to stderr for each one that travels in cleartext
/// ([`print_warnings`]), for a command that runs with no log subscriber.
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
    use super::{CleartextError, CredentialSite, guard};
    use ferrofed_identity::dev::Profile;

    fn site() -> CredentialSite {
        CredentialSite {
            url_key: String::from("xcpd.responding_gateway.url"),
            credential: String::from("xcpd.responding_gateway.credentials"),
        }
    }

    #[test]
    fn https_passes_under_every_profile() {
        for profile in [Profile::Production, Profile::Development] {
            assert_eq!(
                Ok(None),
                guard(profile, "https://pix.example.org/fhir", site())
            );
        }
    }

    #[test]
    fn http_is_refused_outside_development_and_reported_under_it() {
        assert_eq!(
            Err(CleartextError { site: site() }),
            guard(Profile::Production, "http://pix.example.org/fhir", site())
        );
        assert_eq!(
            Ok(Some(site())),
            guard(Profile::Development, "http://pix.example.org/fhir", site())
        );
    }

    #[test]
    fn a_url_that_does_not_parse_counts_as_unprotected() {
        assert_eq!(
            Err(CleartextError { site: site() }),
            guard(Profile::Production, "not a url", site())
        );
    }

    #[test]
    fn the_refusal_names_both_keys_and_never_the_url() {
        let refused = guard(
            Profile::Production,
            "http://user:synthetic-secret@pix.example.org/fhir",
            site(),
        )
        .expect_err("plain http is refused");
        let text = refused.to_string();
        assert!(text.contains("xcpd.responding_gateway.url"), "{text}");
        assert!(
            text.contains("xcpd.responding_gateway.credentials"),
            "{text}"
        );
        assert!(!text.contains("synthetic-secret"), "{text}");
        assert!(!text.contains("pix.example.org"), "{text}");
    }
}
