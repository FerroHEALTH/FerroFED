// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step of `[pdqm]` (Annex A §A.2).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_identity::pdqm::{PdqmConfig, PdqmDemographics};
use openehr_its::rest::client::Credentials;

use crate::config::audit::AuditSettings;
use crate::config::pdqm::PdqmSettings;
use crate::config::settings::Scheme;

use super::DemographicsStep;
use super::error::FederationError;

/// The demographics step `pdqm` describes, audited through `[audit]`.
pub(super) fn pdqm_step(
    pdqm: &PdqmSettings,
    audit: &AuditSettings,
) -> Result<DemographicsStep, FederationError> {
    let credentials = match &pdqm.credentials {
        None => None,
        Some(Scheme::Bearer(token)) => Some(Credentials::bearer(token.to_secret_string())),
        Some(Scheme::Basic { user, password }) => Some(Credentials::basic(
            user.as_str(),
            password.to_secret_string(),
        )),
        Some(Scheme::OAuth2(_) | Scheme::Nuts(_)) => {
            return Err(FederationError::Grant {
                section: String::from("pdqm.credentials"),
            });
        }
    };
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pdqm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(FederationError::PdqmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let step = PdqmDemographics::from_config(PdqmConfig {
        base: pdqm.url.clone(),
        credentials,
        transaction: pdqm.transaction,
        master: pdqm.master.clone(),
        namespaces,
    })
    .map_err(FederationError::Pdqm)?;
    // NOTE: PDQm §2:3.78.5.1 and §2:3.119.5.1.1: each exchange is audited, and one
    // whose record is refused fails, so the patient's resolution fails closed.
    let step = match crate::audit::recorder(audit).map_err(FederationError::Audit)? {
        Some(recorder) => step.audited(&recorder),
        None => step,
    };
    Ok(DemographicsStep::new(Arc::new(step), pdqm.timeout))
}
