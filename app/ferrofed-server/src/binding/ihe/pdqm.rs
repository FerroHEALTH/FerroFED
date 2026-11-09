// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step (Annex A §A.2 and §A.7), built from the
//! `[pdqm]` settings [`crate::config::ihe::pdqm`] resolves.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::ihe::pdqm::{PdqmConfig, PdqmDemographics};
use ferrofed_identity::role::patient::IdentifierNamespace;

use crate::binding::error::BindingError;
use crate::binding::seam::DemographicsStep;
use crate::config::audit::AuditSettings;
use crate::config::ihe::pdqm::PdqmSettings;
use crate::config::service;

/// The demographics step `pdqm` describes, audited through `[audit]`.
pub(super) fn step(
    pdqm: &PdqmSettings,
    audit: &AuditSettings,
) -> Result<DemographicsStep, BindingError> {
    let auth = service::service_authentication("pdqm.credentials", pdqm.credentials.as_ref())?;
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pdqm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(BindingError::PdqmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let step = PdqmDemographics::from_config(PdqmConfig {
        base: pdqm.url.clone(),
        auth,
        tls: Arc::clone(&pdqm.tls),
        transaction: pdqm.transaction,
        master: pdqm.master.clone(),
        namespaces,
    })
    .map_err(BindingError::Pdqm)?;
    // NOTE: PDQm §2:3.78.5.1 and §2:3.119.5.1.1: each exchange is audited, and one
    // whose record is refused fails, so the patient's resolution fails closed.
    let step = match crate::audit::recorder(audit).map_err(BindingError::Audit)? {
        Some(recorder) => step.audited(&recorder),
        None => step,
    };
    Ok(DemographicsStep::new(Arc::new(step), pdqm.timeout))
}
