// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step of `[pdqm]` (Annex A §A.2).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_identity::pdqm::{PdqmConfig, PdqmDemographics};

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
    let auth = match &pdqm.credentials {
        None => Authentication::None,
        Some(Scheme::Bearer(token)) => Authentication::Bearer(token.to_secret_string()),
        Some(Scheme::Basic { user, password }) => Authentication::Basic {
            user: user.clone(),
            password: password.to_secret_string(),
        },
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
        auth,
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
