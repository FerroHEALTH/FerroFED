// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter of `[nl_gf.mitz]`, built over the registry's
//! members (Annex B §B.6, N27a).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ferrofed_identity::consent::ConsentPrefilter;
use ferrofed_identity::dev::Profile;
use ferrofed_identity::mitz::{HolderConfig, MitzConfig, MitzPrefilter};
use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::Secret;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_its::rest::client::Credentials;

use crate::config::mitz::{MITZ_KEY, MitzSettings};
use crate::config::settings::{Scheme, Settings};

use super::error::FederationError;

/// The Mitz pre-filter `[nl_gf.mitz]` describes over the members of
/// `snapshot`, with each holder's URA also read from `[nl_gf.nvi.custodians]`
/// when that table is set; `None` when `[nl_gf.mitz]` is not.
pub(super) fn mitz_prefilter(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
    let nl_gf = settings.nl_gf.as_ref();
    let Some(mitz) = nl_gf.and_then(|nl_gf| nl_gf.mitz.as_ref()) else {
        return Ok(None);
    };
    let mut custodians = BTreeMap::new();
    for (ura, member) in nl_gf
        .and_then(|nl_gf| nl_gf.nvi.as_ref())
        .map(|nvi| &nvi.custodians)
        .into_iter()
        .flatten()
    {
        custodians.insert(
            ura.clone(),
            node(&format!("nl_gf.nvi.custodians.{ura:?}"), member)?,
        );
    }
    let config = MitzConfig {
        endpoint: mitz.url.clone(),
        development: settings.profile == Profile::Development,
        credentials: credentials(mitz),
        client_identity: mitz.client_identity.as_ref().map(Secret::to_secret_string),
        trust_roots: mitz.trust_roots.clone(),
        namespaces: namespaces(mitz)?,
        categories: mitz.data_categories.clone(),
        purpose: mitz.purpose.clone(),
        holders: holders(mitz)?,
        custodians,
        timeout: mitz.timeout,
    };
    let prefilter = MitzPrefilter::from_config(config, snapshot).map_err(FederationError::Mitz)?;
    Ok(Some(Arc::new(prefilter)))
}

/// The member `value` names, read at `key`.
fn node(key: &str, value: &str) -> Result<NodeId, FederationError> {
    NodeId::new(value).map_err(|source| FederationError::MitzMember {
        key: key.to_owned(),
        source,
    })
}

/// The credential the configuration resolved; an OAuth 2.0 or Nuts grant is refused
/// at load, so none reaches here.
fn credentials(mitz: &MitzSettings) -> Option<Credentials> {
    match &mitz.credentials {
        Some(Scheme::Bearer(token)) => Some(Credentials::bearer(token.to_secret_string())),
        Some(Scheme::Basic { user, password }) => Some(Credentials::basic(
            user.as_str(),
            password.to_secret_string(),
        )),
        Some(Scheme::OAuth2(_) | Scheme::Nuts(_) | Scheme::Fapi2(_)) | None => None,
    }
}

/// The namespaces that stand for the BSN.
fn namespaces(mitz: &MitzSettings) -> Result<BTreeSet<IdentifierNamespace>, FederationError> {
    mitz.namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(FederationError::MitzNamespace)
}

/// Each member's data holder, by member id.
fn holders(mitz: &MitzSettings) -> Result<BTreeMap<NodeId, HolderConfig>, FederationError> {
    let mut holders = BTreeMap::new();
    for (member, holder) in &mitz.holders {
        let key = format!("{MITZ_KEY}.holders.{member:?}");
        holders.insert(
            node(&key, member)?,
            HolderConfig {
                ura: holder.ura.clone(),
                kind: holder.kind.clone(),
            },
        );
    }
    Ok(holders)
}
