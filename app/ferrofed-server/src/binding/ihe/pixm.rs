// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver: the identity binding of N3 over ITI-83 (Annex A.1),
//! built over the registry's members from the `[pixm]` settings
//! [`crate::config::ihe::pixm`] resolves.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::ihe::pixm::{ManagerConfig, PixmResolver};
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ihe_iti::pixm::Invocation;

use crate::binding::error::BindingError;
use crate::config::audit::AuditSettings;
use crate::config::ihe::pixm::{PixmMethod, PixmSettings};

/// The PIXm resolver `[pixm]` describes over the members of `snapshot`.
pub(super) fn resolver(
    pixm: &PixmSettings,
    audit: &AuditSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Arc<PixmResolver>, BindingError> {
    let mut managers = Vec::with_capacity(pixm.managers.len());
    for (index, manager) in pixm.managers.iter().enumerate() {
        let mut members = BTreeMap::new();
        for (key, domain) in &manager.members {
            let member = NodeId::new(key.as_str()).map_err(|source| BindingError::PixmMember {
                manager: index,
                key: key.clone(),
                source,
            })?;
            members.insert(member, domain.clone());
        }
        let client = OutboundClient::build(&manager.endpoint, "pixm.manager.endpoint", &ServerTls)
            .map_err(|source| BindingError::Outbound {
                table: format!("pixm.manager[{index}].endpoint"),
                source: Box::new(source),
            })?;
        managers.push(ManagerConfig {
            client,
            members,
            invocation: match manager.method {
                PixmMethod::Get => Invocation::Get,
                PixmMethod::Post => Invocation::Post,
            },
        });
    }
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pixm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(BindingError::PixmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let resolver =
        PixmResolver::from_config(managers, namespaces, snapshot).map_err(BindingError::Pixm)?;
    // NOTE: PIXm §2:3.83.5.1.1: each ITI-83 exchange is audited, and one whose
    // record is refused fails, so the query fails closed.
    Ok(Arc::new(
        match crate::audit::recorder(audit).map_err(BindingError::Audit)? {
            Some(recorder) => resolver.audited(&recorder),
            None => resolver,
        },
    ))
}
