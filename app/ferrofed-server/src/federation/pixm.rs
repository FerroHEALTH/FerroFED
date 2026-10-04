// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver of `[pixm]`, built over the registry's members.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_identity::pixm::{ManagerConfig, PixAuth, PixmResolver};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::config::audit::AuditSettings;
use crate::config::settings::{PixmSettings, Scheme};

use super::error::FederationError;

/// The PIXm resolver `[pixm]` describes over the members of `snapshot`.
pub(super) fn pixm_resolver(
    pixm: &PixmSettings,
    audit: &AuditSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Arc<PixmResolver>, FederationError> {
    let mut managers = Vec::with_capacity(pixm.managers.len());
    for (index, manager) in pixm.managers.iter().enumerate() {
        let mut members = BTreeMap::new();
        for (key, domain) in &manager.members {
            let member =
                NodeId::new(key.as_str()).map_err(|source| FederationError::PixmMember {
                    manager: index,
                    key: key.clone(),
                    source,
                })?;
            members.insert(member, domain.clone());
        }
        let auth = match &manager.credentials {
            None => PixAuth::None,
            Some(Scheme::Bearer(token)) => PixAuth::Bearer(token.to_secret_string()),
            Some(Scheme::Basic { user, password }) => PixAuth::Basic {
                user: user.clone(),
                password: password.to_secret_string(),
            },
            Some(Scheme::OAuth2(_)) => {
                return Err(FederationError::Grant {
                    section: format!("pixm.manager[{index}].credentials"),
                });
            }
        };
        managers.push(ManagerConfig {
            base: manager.url.clone(),
            auth,
            members,
        });
    }
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pixm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(FederationError::PixmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let resolver =
        PixmResolver::from_config(managers, namespaces, snapshot).map_err(FederationError::Pixm)?;
    // NOTE: PIXm §2:3.83.5.1.1: each ITI-83 exchange is audited, and one whose
    // record is refused fails, so the query fails closed.
    Ok(Arc::new(
        match crate::audit::recorder(audit).map_err(FederationError::Audit)? {
            Some(recorder) => resolver.audited(&recorder),
            None => resolver,
        },
    ))
}
