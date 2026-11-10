// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sections of the Dutch binding, the Generic Functions of Annex B.
//!
//! One table per function under `[nl_gf]`, and the Nuts grant as an onward
//! credential, as the configuration writes them and resolved:
//!
//! - `[nl_gf.nvi]`: the localizer over GF-Localization, the NVI
//!   Localization Service (Annex B §B.1, N4, §14.1) ([`nvi`]);
//! - `[nl_gf.mitz]`: the consent pre-filter over GF-Consent, the closed
//!   authorization question of Mitz (Annex B §B.6, N27a) ([`mitz`]);
//! - `[credentials."<endpoint id>".nuts]`: the Nuts grant a node is reached
//!   with (Annex B §B.4) ([`nuts`]).
//!
//! The Dutch binding builds the localizer, the pre-filter and the node
//! credentials from them. No specification governs the grouping: our own
//! design.

pub mod mitz;
pub mod nuts;
pub mod nvi;

use serde::Deserialize;

use crate::config::error::Error;
use crate::config::settings::{OnwardGrant, Settings};
use crate::config::transport::{self, CleartextError, ProtectedSite};
use crate::config::{Config, NodeCredentials};

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
/// `https` outside development, basic credentials or the Nuts grant for the
/// NVI, basic credentials or the `oauth` client credentials grant for Mitz,
/// and every secret and file read.
///
/// # Errors
/// [`BindingFault::BsnAsPseudonym`] for a BSN system listed in `namespaces`,
/// [`Error::Missing`] for no registry or no `endpoint.url`, [`Error::Url`] for a URL
/// that does not parse, [`Error::UrlCredentials`] for one that carries a
/// user name or a password, [`Error::Cleartext`] for one that is not
/// `https` outside the development profile, [`BindingFault::NviGrant`] for an
/// `oauth` grant for the NVI,
/// [`CredentialsFault::ServiceGrantNotHere`] for a Nuts grant for Mitz, the errors of a Nuts grant that cannot be built, and
/// the errors of a secret or a file that cannot be read.
///
/// [`BindingFault::BsnAsPseudonym`]: crate::config::error::binding::BindingFault::BsnAsPseudonym
/// [`BindingFault::NviGrant`]: crate::config::error::binding::BindingFault::NviGrant
/// [`CredentialsFault::ServiceGrantNotHere`]: crate::config::error::credentials::CredentialsFault::ServiceGrantNotHere
fn resolve_nl_gf(config: &Config) -> Result<Option<NlGfSettings>, Error> {
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

/// Resolves `[nl_gf]` into `settings`.
///
/// # Errors
///
/// The errors of `[nl_gf.nvi]` and `[nl_gf.mitz]`.
pub(crate) fn resolve(config: &Config, settings: &mut Settings) -> Result<(), Error> {
    settings.nl_gf = resolve_nl_gf(config)?;
    Ok(())
}

/// Whether `credentials` sets the Nuts grant, `[credentials."<id>".nuts]`.
pub(crate) fn sets_onward(credentials: &NodeCredentials) -> bool {
    credentials.nuts.is_some()
}

/// Resolves the Nuts grant `credentials` sets under `section`, when it sets
/// one.
pub(crate) fn onward(
    section: &str,
    credentials: &NodeCredentials,
) -> Option<Result<Box<dyn OnwardGrant>, Error>> {
    credentials.nuts.as_ref().map(|table| {
        nuts::resolve(&format!("{section}.{}", nuts::KEY), table)
            .map(|grant| -> Box<dyn OnwardGrant> { Box::new(grant) })
    })
}

/// Holds the sites the binding's services are reached at to the transport
/// policy, and returns those that travel in cleartext under the development
/// profile.
///
/// # Errors
///
/// The [`CleartextError`] of the first site the policy refuses.
pub(crate) fn sites(settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
    let Some(nl_gf) = &settings.nl_gf else {
        return Ok(Vec::new());
    };
    let services = [
        nl_gf.nvi.as_ref().map(|nvi| {
            (
                nvi::NVI_ENDPOINT,
                nvi.url.as_str(),
                nvi.credentials.is_some(),
            )
        }),
        nl_gf.mitz.as_ref().map(|mitz| {
            (
                mitz::MITZ_ENDPOINT,
                mitz.url.as_str(),
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
    // NOTE: Nuts RFC021 §7, every endpoint is TLS-protected, the NVI's
    // authorization server as a node's.
    if let Some((url, site)) = nl_gf.nvi.as_ref().and_then(nvi::NviSettings::grant_site) {
        cleartext.extend(transport::protected_payload(settings.profile, &url, site)?);
    }
    Ok(cleartext)
}

/// Logs what the binding is configured to reach, never a value.
pub(crate) fn log_summary(settings: &Settings) {
    let nl_gf = settings.nl_gf.as_ref();
    tracing::info!(
        binding = "nl",
        nvi_localizer = nl_gf.is_some_and(|nl_gf| nl_gf.nvi.is_some()),
        mitz_prefilter = nl_gf.is_some_and(|nl_gf| nl_gf.mitz.is_some()),
        "binding configured"
    );
}
