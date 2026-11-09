// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter of the Dutch Generic Functions (Annex B
//! §B.2), built from the `[nl_gf.mitz]` settings `crate::config::nl::mitz`
//! resolves.

use crate::binding::error::BindingError;
use crate::config::nl::NlGfSettings;
use crate::config::nl::mitz::{MITZ_KEY, MitzSettings};
use crate::config::service;
use crate::config::settings::Settings;
use ferrofed_identity::nl::mitz::{HolderConfig, MitzConfig, MitzPrefilter};
use ferrofed_identity::profile::Profile;
use ferrofed_identity::role::consent::ConsentPrefilter;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The Mitz pre-filter `[nl_gf.mitz]` describes over the members of
/// `snapshot`, with each holder's URA also read from `[nl_gf.nvi.custodians]`
/// when that table is set; `None` when `[nl_gf.mitz]` is not.
pub(super) fn prefilter(
    settings: &Settings,
    nl_gf: &NlGfSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<Arc<dyn ConsentPrefilter>>, BindingError> {
    let Some(mitz) = &nl_gf.mitz else {
        return Ok(None);
    };
    let mut custodians = BTreeMap::new();
    for (ura, member) in nl_gf
        .nvi
        .as_ref()
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
        auth: service::service_authentication(
            &format!("{MITZ_KEY}.credentials"),
            mitz.credentials.as_ref(),
        )?,
        tls: Arc::clone(&mitz.tls),
        namespaces: namespaces(mitz)?,
        categories: mitz.data_categories.clone(),
        purpose: mitz.purpose.clone(),
        holders: holders(mitz)?,
        custodians,
        timeout: mitz.timeout,
    };
    let prefilter = MitzPrefilter::from_config(config, snapshot).map_err(BindingError::Mitz)?;
    Ok(Some(Arc::new(prefilter)))
}

/// The member `value` names, read at `key`.
fn node(key: &str, value: &str) -> Result<NodeId, BindingError> {
    NodeId::new(value).map_err(|source| BindingError::MitzMember {
        key: key.to_owned(),
        source,
    })
}

/// The namespaces that stand for the BSN.
fn namespaces(mitz: &MitzSettings) -> Result<BTreeSet<IdentifierNamespace>, BindingError> {
    mitz.namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(BindingError::MitzNamespace)
}

/// Each member's data holder, by member id.
fn holders(mitz: &MitzSettings) -> Result<BTreeMap<NodeId, HolderConfig>, BindingError> {
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
