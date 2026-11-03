// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sections of the IHE binding of Annex A.
//!
//! They are `[pixm]` (Annex A.1), `[pdqm]` (§A.2), `[xcpd]` with its ITI-55
//! audit repository (Annex A.3), `[pmir]` (Annex A.4) and `[registry.mcsd]`
//! (Annex A.5), as the configuration writes them and resolved, with the
//! sites they are reached at and the summary line of what they configure.
//! The IHE binding builds the resolver, the demographics step, the
//! localizer, the identity feed and the directory reader from them. No
//! specification governs the grouping: our own design.

pub mod audit_repository;
pub mod mcsd;
pub mod pdqm;
pub mod pixm;
pub mod pmir;
pub mod xcpd;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::service_grant::{self};
use crate::config::settings::{Scheme, Settings};
use crate::config::transport::{self, CleartextError, ProtectedSite};

/// Whether a section of the binding that `config` sets records its
/// transactions through `[audit]`, which then needs a destination outside
/// development.
pub(crate) fn audits(config: &Config) -> bool {
    config.pixm.is_some()
        || config.pdqm.is_some()
        || config.pmir.is_some()
        || config.registry.mcsd.is_some()
}

/// Holds the sites the binding's services are reached at to the transport
/// policy, and returns those that travel in cleartext under the development
/// profile.
///
/// # Errors
///
/// The [`CleartextError`] of the first site the policy refuses.
pub(crate) fn sites(settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
    let profile = settings.profile;
    let mut cleartext = Vec::new();
    let mut hold = |url: &str, site: ProtectedSite| {
        transport::protected_payload(profile, url, site).map(|exposed| cleartext.extend(exposed))
    };
    for (index, manager) in settings
        .pixm
        .iter()
        .flat_map(|pixm| pixm.managers.iter().enumerate())
    {
        let key = format!("pixm.manager[{index}]");
        let credentials = manager
            .credentials
            .is_some()
            .then(|| format!("{key}.credentials"));
        hold(
            manager.url.expose(),
            transport::identity_site(&key, credentials.as_deref()),
        )?;
    }
    if let Some(pdqm) = &settings.pdqm {
        let key = pdqm::PDQM_KEY;
        let credentials = pdqm
            .credentials
            .is_some()
            .then(|| format!("{key}.credentials"));
        hold(
            pdqm.url.expose(),
            transport::identity_site(key, credentials.as_deref()),
        )?;
    }
    let mut audit = None;
    if let Some(xcpd) = &settings.xcpd {
        let assertion = xcpd.assertion.as_ref().map(|_| xcpd.assertion_key);
        for (index, gateway) in xcpd.gateways.iter().enumerate() {
            let key = format!("xcpd.gateway[{index}]");
            hold(
                gateway.url.expose(),
                transport::identity_site(&key, assertion),
            )?;
        }
        if let Some(repository) = &xcpd.audit_repository {
            let site = ProtectedSite {
                url_key: String::from("xcpd.audit_repository.url"),
                payload: String::from("the ITI-55 audit messages, which name the patient"),
                requires: transport::Encryption::SyslogTls,
            };
            audit = transport::encrypted_syslog(profile, &repository.url, site)?;
        }
    }
    if let Some(directory) = settings
        .registry_directory
        .as_ref()
        .filter(|directory| directory.credentials.is_some())
    {
        hold(directory.url.expose(), mcsd::site())?;
    }
    for (url, site) in token_endpoints(settings) {
        hold(&url, site)?;
    }
    cleartext.extend(pmir::sites(profile, settings.pmir.as_ref())?);
    cleartext.extend(audit);
    Ok(cleartext)
}

/// Logs what the binding is configured to reach, never a value.
pub(crate) fn log_summary(settings: &Settings) {
    tracing::info!(
        binding = "ihe",
        registry_directory = settings.registry_directory.is_some(),
        registry_refresh_s = settings
            .registry_directory
            .as_ref()
            .map(|directory| directory.refresh_interval.as_secs()),
        pix_managers = settings.pixm.as_ref().map_or(0, |pixm| pixm.managers.len()),
        pmir_feed = settings.pmir.is_some(),
        pdqm_transaction = settings.pdqm.as_ref().map(|pdqm| pdqm.transaction.as_str()),
        xcpd_localizer = settings.xcpd.is_some(),
        "binding configured"
    );
}

/// Resolves the IHE binding's sections of `config` into `settings`: `[pixm]`,
/// `[xcpd]`, `[pmir]`, `[registry.mcsd]` and `[pdqm]`.
pub(crate) fn resolve(config: &Config, settings: &mut Settings) -> Result<(), Error> {
    use crate::config::error::federation::FederationFault;
    use crate::config::service_grant::ServiceContext;
    settings.pixm = config
        .pixm
        .as_ref()
        .map(|section| pixm::resolve(section, ServiceContext::of(settings)))
        .transpose()?;
    settings.xcpd = xcpd::resolve(config)?;
    settings.pmir = pmir::resolve(
        config,
        ServiceContext::of(settings),
        settings.server.public_url.as_ref(),
        settings.server.auth.basic.as_ref(),
    )?;
    if let (Some(fhir), Some(pmir)) = (&settings.fhir, &settings.pmir) {
        let route = settings.server.base_path.join(&pmir.path);
        crate::config::fhir::apart(fhir, &route, "pmir.path")?;
    }
    if config.registry.document.is_some() && config.registry.mcsd.is_some() {
        return Err(Error::Federation(FederationFault::TwoRegistrySources));
    }
    settings.registry_directory = config
        .registry
        .mcsd
        .as_ref()
        .map(|directory| {
            mcsd::resolve(
                directory,
                ServiceContext::of(settings),
                settings.audit.clone(),
            )
        })
        .transpose()?;
    settings.pdqm = pdqm::resolve(config, ServiceContext::of(settings))?;
    Ok(())
}

/// The token endpoint of every IHE FHIR service whose credentials are a
/// client-credentials grant, with its site: the gateway's client
/// credentials travel to it (RFC 6749 §2.3.1).
fn token_endpoints(settings: &Settings) -> Vec<(String, ProtectedSite)> {
    let pixm = settings
        .pixm
        .iter()
        .flat_map(|pixm| pixm.managers.iter().enumerate())
        .map(|(index, manager)| {
            (
                format!("pixm.manager[{index}].credentials"),
                manager.credentials.as_ref(),
            )
        });
    let pdqm = settings.pdqm.iter().map(|pdqm| {
        (
            format!("{}.credentials", pdqm::PDQM_KEY),
            pdqm.credentials.as_ref(),
        )
    });
    let directory = settings.registry_directory.iter().map(|directory| {
        (
            String::from("registry.mcsd.credentials"),
            directory.credentials.as_ref(),
        )
    });
    let feed = settings
        .pmir
        .iter()
        .map(|pmir| (String::from("pmir.credentials"), pmir.credentials.as_ref()));
    pixm.chain(pdqm)
        .chain(directory)
        .chain(feed)
        .filter_map(|(section, scheme)| match scheme {
            Some(Scheme::Oauth { grant, .. }) => {
                crate::config::oauth::token_endpoint(grant).map(|endpoint| {
                    (
                        endpoint.to_string(),
                        service_grant::site(&format!("{section}.oauth")),
                    )
                })
            }
            _ => None,
        })
        .collect()
}
