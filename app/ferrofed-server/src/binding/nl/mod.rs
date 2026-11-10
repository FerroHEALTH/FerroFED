// SPDX-FileCopyrightText: Cadasto B.V.
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

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::role::consent::ConsentPrefilter;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::error::BindingError;
use crate::binding::seam::LocalizationError;
use crate::binding::seam::{LocalizerSeam, ProvideError, Provided, PublicDocument};
use crate::binding::{Binding, Offer, Reload, Role, Section};
use crate::config::nl::nuts::NutsOnward;
use crate::config::settings::OnwardGrant;
use crate::config::settings::Settings;
use crate::config::{EmergencyConsent, Localization};

/// The Dutch binding.
#[derive(Debug, Clone, Copy)]
pub struct Nl;

/// The sections the Dutch binding reads.
const SECTIONS: &[Section] = &[Section {
    key: "nl_gf",
    reload: Reload::Applies,
}];

impl Binding for Nl {
    fn name(&self) -> &'static str {
        "nl"
    }

    fn sections(&self) -> &'static [Section] {
        SECTIONS
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
        let timeout = settings.federation.localization.map_or_else(
            || Duration::from_millis(Localization::default().timeout.as_millis()),
            |localization| localization.timeout,
        );
        settings
            .nl_gf
            .as_ref()
            .and_then(|nl_gf| nl_gf.nvi.as_ref())
            .map(|section| nvi::localizer(section, snapshot, timeout))
            .transpose()
    }

    fn prefilter(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<Arc<dyn ConsentPrefilter>>, BindingError> {
        match &settings.nl_gf {
            Some(nl_gf) => mitz::prefilter(settings, nl_gf, snapshot),
            None => Ok(None),
        }
    }

    fn documents(&self, settings: &Settings) -> Result<Vec<PublicDocument>, BindingError> {
        nuts::documents(settings)
    }

    fn provide(
        &self,
        grant: &dyn OnwardGrant,
        endpoint: &EndpointId,
        settings: &Settings,
    ) -> Option<Result<Provided, ProvideError>> {
        let grant: &dyn Any = grant;
        grant
            .downcast_ref::<NutsOnward>()
            .map(|nuts| nuts::provide(nuts, endpoint, settings))
    }

    fn class(&self, error: &BindingError) -> Option<&'static str> {
        match error {
            BindingError::MitzMember { .. }
            | BindingError::MitzNamespace(_)
            | BindingError::Mitz(_) => Some("consent-prefilter"),
            _ => None,
        }
    }

    fn notes(&self, settings: &Settings) -> Vec<String> {
        let mitz = settings
            .nl_gf
            .as_ref()
            .is_some_and(|nl_gf| nl_gf.mitz.is_some());
        // NOTE: Wabvpz Art 15a has no emergency exception and Mitz 3.8.2 §3.2.4.2 admits only
        // TREAT and COC, so passing an emergency on is the operator's legal question to answer.
        if mitz && settings.federation.consent_emergency == EmergencyConsent::PassToNode {
            vec![String::from(
                "[federation.consent] emergency = \"pass-to-node\" with [nl_gf.mitz]: Wabvpz Art 15a has no emergency exception and Mitz 3.8.2 has no emergency consultation situation, so a member Mitz denies is asked under a legal basis of your own, which the node applies",
            )]
        } else {
            Vec::new()
        }
    }
}
