// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch binding, the Generic Functions of Annex B: one table per
//! function under `[nl_gf]`, and the Nuts grant as an onward credential.
//!
//! - `[nl_gf.nvi]`: the localizer over GF-Localization, the NVI
//!   Localization Service (Annex B §B.1, N4, §14.1) ([`nvi`]);
//! - `[nl_gf.mitz]`: the consent pre-filter over GF-Consent, the closed
//!   authorization question of Mitz (Annex B §B.6, N27a) ([`mitz`]);
//! - `[credentials."<endpoint id>".nuts]`: the Nuts grant a node is reached
//!   with (Annex B §B.4) ([`nuts`]).
//!
//! The URA each member's organisation carries, with the LRZa as its source
//! (Annex B §B.2), is read by the NVI and Mitz adapters from the registry.
//! No specification governs the grouping: our own design.

pub mod mitz;
pub mod nuts;
pub mod nvi;

use std::sync::Arc;

use ferrofed_identity::consent::ConsentPrefilter;
use ferrofed_registry::snapshot::RegistrySnapshot;
use serde::Deserialize;

use crate::binding::{
    Binding, LocalizerSeam, Offer, OnwardGrant, Reload, Role, Section, StepBudgets,
};
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, ProtectedSite};
use crate::config::{Config, Credentials};
use crate::federation::error::FederationError;
use crate::localization::LocalizationError;

/// The Dutch binding.
#[derive(Debug, Clone, Copy)]
pub struct Nl;

/// The sections the Dutch binding reads.
const SECTIONS: &[Section] = &[Section {
    key: "nl_gf",
    reload: Reload::Applies,
}];

/// The Dutch Generic Functions, as the configuration writes them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NlGf {
    /// The NVI localizer (`[nl_gf.nvi]`).
    pub nvi: Option<nvi::Nvi>,
    /// The Mitz consent pre-filter (`[nl_gf.mitz]`).
    pub mitz: Option<mitz::Mitz>,
}

/// The Dutch Generic Functions, resolved.
#[derive(Debug)]
pub struct NlGfSettings {
    /// The NVI localizer, when `[nl_gf.nvi]` is set.
    pub nvi: Option<nvi::NviSettings>,
    /// The Mitz consent pre-filter, when `[nl_gf.mitz]` is set.
    pub mitz: Option<mitz::MitzSettings>,
}

/// Resolves `[nl_gf]`: an NVI URL that parses, carries no userinfo and is
/// `https` outside development, a bearer token or basic credentials, and
/// every secret and file read.
///
/// # Errors
/// [`Error::BsnAsPseudonym`] for a BSN system listed in `namespaces`,
/// [`Error::Missing`] for no registry or no `url`, [`Error::Url`] for a URL
/// that does not parse, [`Error::UrlCredentials`] for one that carries a
/// user name or a password, [`Error::Cleartext`] for one that is not
/// `https` outside the development profile, [`Error::GrantNotHere`] for an
/// OAuth 2.0 grant, and the errors of a secret or a file that cannot be read.
fn resolve(config: &Config) -> Result<Option<NlGfSettings>, Error> {
    let Some(nl_gf) = &config.nl_gf else {
        return Ok(None);
    };
    let nvi = nl_gf
        .nvi
        .as_ref()
        .map(|nvi| nvi::resolve(config, nvi))
        .transpose()?;
    let mitz = nl_gf
        .mitz
        .as_ref()
        .map(|mitz| mitz::resolve(config, mitz))
        .transpose()?;
    Ok(Some(NlGfSettings { nvi, mitz }))
}

impl Binding for Nl {
    fn name(&self) -> &'static str {
        "nl"
    }

    fn sections(&self) -> &'static [Section] {
        SECTIONS
    }

    fn resolve(&self, config: &Config, settings: &mut Settings) -> Result<(), Error> {
        settings.nl_gf = resolve(config)?;
        Ok(())
    }

    fn budgets(&self, config: &Config) -> StepBudgets {
        StepBudgets {
            demographics_ms: 0,
            consent_ms: config
                .nl_gf
                .as_ref()
                .and_then(|nl_gf| nl_gf.mitz.as_ref())
                .map_or(0, |mitz| mitz.timeout_ms),
        }
    }

    fn offers(&self, settings: &Settings) -> Vec<Offer> {
        let Some(nl_gf) = &settings.nl_gf else {
            return Vec::new();
        };
        let offered = [
            (nl_gf.nvi.is_some(), Role::Localizer, "[nl_gf.nvi]"),
            (nl_gf.mitz.is_some(), Role::ConsentPrefilter, "[nl_gf.mitz]"),
        ];
        offered
            .into_iter()
            .filter_map(|(set, role, section)| set.then_some(Offer { role, section }))
            .collect()
    }

    fn localizers(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&["[nl_gf.nvi]"], &[])
    }

    fn localizer(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<LocalizerSeam>, LocalizationError> {
        settings
            .nl_gf
            .as_ref()
            .and_then(|nl_gf| nl_gf.nvi.as_ref())
            .map(|section| nvi::localizer(section, snapshot))
            .transpose()
    }

    fn prefilter(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
        match &settings.nl_gf {
            Some(nl_gf) => mitz::prefilter(settings, nl_gf, snapshot),
            None => Ok(None),
        }
    }

    fn onward_table(&self, credentials: &Credentials) -> Option<&'static str> {
        credentials.nuts.as_ref().map(|_| nuts::KEY)
    }

    fn onward(
        &self,
        section: &str,
        credentials: &Credentials,
    ) -> Option<Result<Box<dyn OnwardGrant>, Error>> {
        credentials.nuts.as_ref().map(|table| {
            nuts::resolve(&format!("{section}.{}", nuts::KEY), table)
                .map(|grant| -> Box<dyn OnwardGrant> { Box::new(grant) })
        })
    }

    fn sites(&self, settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
        let Some(nl_gf) = &settings.nl_gf else {
            return Ok(Vec::new());
        };
        let services = [
            nl_gf
                .nvi
                .as_ref()
                .map(|nvi| (nvi::NVI_KEY, nvi.url.expose(), nvi.credentials.is_some())),
            nl_gf.mitz.as_ref().map(|mitz| {
                (
                    mitz::MITZ_KEY,
                    mitz.url.expose(),
                    mitz.credentials.is_some(),
                )
            }),
        ];
        let mut cleartext = Vec::new();
        for (key, url, credentialed) in services.into_iter().flatten() {
            let credentials = credentialed.then(|| format!("{key}.credentials"));
            cleartext.extend(transport::protected_payload(
                settings.profile,
                url,
                transport::identity_site(key, credentials.as_deref()),
            )?);
        }
        Ok(cleartext)
    }

    fn class(&self, error: &FederationError) -> Option<&'static str> {
        match error {
            FederationError::MitzMember { .. }
            | FederationError::MitzNamespace(_)
            | FederationError::Mitz(_) => Some("consent-prefilter"),
            FederationError::NutsClient { .. } => Some("http-client"),
            _ => None,
        }
    }

    fn log_summary(&self, settings: &Settings) {
        let nl_gf = settings.nl_gf.as_ref();
        tracing::info!(
            binding = self.name(),
            nvi_localizer = nl_gf.is_some_and(|nl_gf| nl_gf.nvi.is_some()),
            mitz_prefilter = nl_gf.is_some_and(|nl_gf| nl_gf.mitz.is_some()),
            "binding configured"
        );
    }
}
