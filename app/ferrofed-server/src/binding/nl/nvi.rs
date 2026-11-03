// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer of the Dutch Generic Functions (Annex B §B.1), built
//! from the `[nl_gf.nvi]` settings `crate::config::nl::nvi` resolves.

use super::nuts;
use crate::binding::seam::LocalizerSeam;
use crate::config::nl::nvi::{CREDENTIALS_KEY, NVI_KEY, NviCredentials, NviSettings};
use crate::config::service;
use crate::localization::LocalizationError;
use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::grant::nl::nuts::NutsAuthorizer;
use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::nl::nvi::{NviConfig, NviLocalizer};
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use nl_generic_functions::authorizer::Authorizer;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

/// The `localization.mode` of the NVI localizer of the Dutch Generic
/// Functions (Annex B §B.1).
pub const NL_GF_NVI: &str = "nl-gf-nvi";

/// The NVI localizer `nvi` describes over the members of `snapshot`
/// (Annex B §B.1), a Nuts grant's token requests bounded by `timeout`.
pub(super) fn localizer(
    nvi: &NviSettings,
    snapshot: &RegistrySnapshot,
    timeout: Duration,
) -> Result<LocalizerSeam, LocalizationError> {
    let mut custodians = BTreeMap::new();
    for (ura, member) in &nvi.custodians {
        let member =
            NodeId::new(member.as_str()).map_err(|source| LocalizationError::NviMember {
                ura: ura.clone(),
                source,
            })?;
        custodians.insert(ura.clone(), member);
    }
    let namespaces = nvi
        .namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(LocalizationError::NviNamespace)?;
    let (auth, authorizer) = match &nvi.credentials {
        None => (Authentication::None, None),
        Some(NviCredentials::Header(scheme)) => (
            service::authentication(CREDENTIALS_KEY, Some(scheme))
                .map_err(LocalizationError::Grant)?,
            None,
        ),
        Some(NviCredentials::Nuts(grant)) => {
            let client = nuts::http_client().map_err(LocalizationError::NviNutsClient)?;
            let authorizer: Arc<dyn Authorizer> = Arc::new(NutsAuthorizer::new(
                NVI_KEY,
                grant.grant().clone(),
                client,
                timeout,
                Arc::new(SystemClock),
            ));
            (Authentication::None, Some(authorizer))
        }
    };
    let tls = service::tls(
        NVI_KEY,
        nvi.client_identity.as_ref(),
        nvi.trust_roots.as_deref(),
    )
    .map_err(LocalizationError::Tls)?;
    let config = NviConfig {
        base: nvi.url.clone(),
        auth,
        authorizer,
        custodians,
        namespaces,
        tls,
    };
    let localizer = NviLocalizer::from_config(config, snapshot).map_err(LocalizationError::Nvi)?;
    Ok(LocalizerSeam {
        localizer: Arc::new(localizer),
        mode: NL_GF_NVI,
        audit: None,
        indicators: Vec::new(),
    })
}
