// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The regional and national bindings: one module per binding, each
//! implementing [`Binding`], and the generic wiring that builds the gateway's
//! roles from every binding this build compiles.
//!
//! The Federation Tier's core carries no national concept. A deployment fills
//! the roles the specification names with the services of its region:
//! resolution (§5.2, N3), localization (§14, N4), the consent pre-filter
//! (§13.2.1, N27a), addressing (§8, §15) and the authentication to each node
//! (§13). A binding declares which of those roles its configuration sections
//! fill ([`Binding::offers`]), and builds each one it fills. The server then
//! holds the role rules once, for every binding: at most one resolver, at
//! most one consent pre-filter, and at most one localizer of its own, beside
//! the resolver that also localizes ([`single`], [`RoleConflict`]).
//!
//! A binding also declares the configuration sections it reads and what a
//! reload does with each ([`Binding::sections`]), the sites its
//! services are reached at for the transport policy ([`Binding::sites`]), the
//! health indicators of what it runs ([`seam::Indicator`]), and the onward
//! credential kinds it adds ([`seam::OnwardGrant`]). Its self-description is the
//! mode each role it builds carries, which `OPTIONS {base}/` declares from
//! the running roles. No specification governs the module layout: our own
//! design.

pub mod development;
#[cfg(feature = "binding-ihe")]
pub mod ihe;
#[cfg(feature = "binding-nl")]
pub mod nl;
pub mod process;
pub mod seam;

use std::fmt;
use std::sync::Arc;

use ferrofed_identity::consent::ConsentPrefilter;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::seam::{Indicator, LocalizerSeam, OnwardGrant, PublicDocument, ResolverSeam};
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::config::transport::{CleartextError, ProtectedSite};
use crate::config::{Config, Credentials};
use crate::federation::DemographicsStep;
use crate::federation::error::FederationError;
use crate::localization::LocalizationError;

/// Every binding this build compiles, in the order the gateway wires them.
///
/// A regional binding comes before the international one, so its sections
/// are resolved and reported first, and the development binding comes last:
/// its cross-reference is the localizer of last resort, under the
/// development profile alone.
static COMPILED: &[&dyn Binding] = &[
    #[cfg(feature = "binding-nl")]
    &nl::Nl,
    #[cfg(feature = "binding-ihe")]
    &ihe::Ihe,
    &development::Development,
];

/// The configuration sections every reload applies whatever the bindings:
/// the registry and the onward credentials of each endpoint.
const CORE_RELOADABLE: [&str; 2] = ["registry", "credentials"];

/// Returns every binding this build compiles, in wiring order.
#[must_use]
pub fn compiled() -> &'static [&'static dyn Binding] {
    COMPILED
}

/// A role the Federation Tier names, which a binding's section can fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Role {
    /// Under which local `ehr_id` each member knows the patient (§5.2, N3).
    Resolver,
    /// Where the patient's data may be (§14, N4).
    Localizer,
    /// Which master identity a patient identifier the cross-reference does
    /// not map names (Annex A §A.2).
    Demographics,
    /// Which candidates may not be asked about the patient (§13.2.1, N27a).
    ConsentPrefilter,
    /// Where the registry of members is read from (§8, §15).
    RegistrySource,
    /// What tells the gateway a patient's identity changed (§5.2, track 8).
    IdentityFeed,
}

impl Role {
    /// Returns the role's name in the plural, as an error names it.
    #[must_use]
    pub const fn plural(self) -> &'static str {
        match self {
            Self::Resolver => "resolvers",
            Self::Localizer => "localizers",
            Self::Demographics => "demographics steps",
            Self::ConsentPrefilter => "consent pre-filters",
            Self::RegistrySource => "registry sources",
            Self::IdentityFeed => "identity feeds",
        }
    }
}

/// A role one configured section fills, read from the settings without
/// building anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// The role.
    pub role: Role,
    /// The section that fills it, as an error names it: `[xcpd]`.
    pub section: &'static str,
}

/// One configuration section a binding reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    /// The section's key, such as `pixm` or `xcpd.audit`.
    pub key: &'static str,
    /// What a registry reload does with a change to it.
    pub reload: Reload,
}

/// What a registry reload does with a changed section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reload {
    /// The reload applies the change.
    Applies,
    /// The change takes a restart; the reload keeps the running value and
    /// names the key.
    Restart,
}

/// The budgets of the Step-1 services a binding's sections ask, each a part
/// of the overall budget (§11.5), in milliseconds; zero for a service it
/// does not configure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepBudgets {
    /// The demographics step's budget.
    pub demographics_ms: u64,
    /// The consent pre-filter's budget.
    pub consent_ms: u64,
}

/// More than one configured section fills a role exactly one may.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{} are {} {}; set one",
    listed(&self.sections),
    if self.sections.len() > 2 { "all" } else { "both" },
    self.role.plural()
)]
pub struct RoleConflict {
    /// The role.
    pub role: Role,
    /// The sections that each fill it, in wiring order.
    pub sections: Vec<&'static str>,
}

/// `sections` as one phrase: `[a] and [b]`, or `[a], [b] and [c]`.
fn listed(sections: &[&'static str]) -> String {
    match sections.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => (*last).to_owned(),
        None => String::new(),
    }
}

/// One regional or national binding.
///
/// Every method but [`Binding::name`], [`Binding::sections`],
/// [`Binding::resolve`] and [`Binding::offers`] has a default that fills no
/// role, so a binding implements the hooks of the roles it fills.
pub trait Binding: fmt::Debug + Sync {
    /// Returns the binding's name, which its Cargo feature carries.
    fn name(&self) -> &'static str;

    /// Returns the configuration sections the binding reads, each with what
    /// a registry reload does with it.
    fn sections(&self) -> &'static [Section];

    /// Resolves the binding's sections of `config` into `settings`.
    ///
    /// # Errors
    ///
    /// The [`Error`] of the first value the binding refuses, under the key
    /// that carries it.
    fn resolve(&self, config: &Config, settings: &mut Settings) -> Result<(), Error>;

    /// Returns the budgets of the Step-1 services the binding's sections of
    /// `config` ask.
    fn budgets(&self, _config: &Config) -> StepBudgets {
        StepBudgets::default()
    }

    /// Returns the roles the binding's configured sections fill, read from
    /// `settings` without building anything.
    fn offers(&self, settings: &Settings) -> Vec<Offer>;

    /// Returns how a missing localizer names the binding's localizers: those
    /// of its own first, then the resolvers that localize.
    fn localizers(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&[], &[])
    }

    /// Returns the sections of the binding that configure a cross-reference
    /// resolver, as an error that needs one names them.
    fn resolvers(&self) -> &'static [&'static str] {
        &[]
    }

    /// Refuses a section that names registry members when no registry is
    /// configured.
    ///
    /// # Errors
    ///
    /// The binding's [`FederationError`] naming the section.
    fn unregistered(&self, _settings: &Settings) -> Result<(), FederationError> {
        Ok(())
    }

    /// Builds the binding's resolver over `snapshot`, when it offers one.
    ///
    /// # Errors
    ///
    /// The [`FederationError`] of a resolver that refuses its configuration.
    fn resolver(
        &self,
        _settings: &Settings,
        _snapshot: &RegistrySnapshot,
    ) -> Result<Option<ResolverSeam>, FederationError> {
        Ok(None)
    }

    /// Builds the binding's demographics step, `resolving` saying whether a
    /// resolver takes the master identity it finds.
    ///
    /// # Errors
    ///
    /// The [`FederationError`] of a step that refuses its configuration or
    /// that no resolver follows.
    fn demographics(
        &self,
        _settings: &Settings,
        _resolving: bool,
    ) -> Result<Option<DemographicsStep>, FederationError> {
        Ok(None)
    }

    /// Builds the binding's own localizer over `snapshot`, when it offers
    /// one.
    ///
    /// # Errors
    ///
    /// The [`LocalizationError`] of a localizer that refuses its
    /// configuration.
    fn localizer(
        &self,
        _settings: &Settings,
        _snapshot: &RegistrySnapshot,
    ) -> Result<Option<LocalizerSeam>, LocalizationError> {
        Ok(None)
    }

    /// Builds the binding's consent pre-filter over `snapshot`, when it
    /// offers one.
    ///
    /// # Errors
    ///
    /// The [`FederationError`] of a pre-filter that refuses its
    /// configuration.
    fn prefilter(
        &self,
        _settings: &Settings,
        _snapshot: &RegistrySnapshot,
    ) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
        Ok(None)
    }

    /// Returns the health indicators of what a federation records through,
    /// such as an audit trail.
    ///
    /// # Errors
    ///
    /// The [`FederationError`] of a trail that cannot start.
    fn indicators(&self, _settings: &Settings) -> Result<Vec<Arc<dyn Indicator>>, FederationError> {
        Ok(Vec::new())
    }

    /// Returns the public documents the binding's configured sections have
    /// the gateway serve, built from `settings` alone.
    ///
    /// # Errors
    ///
    /// The [`FederationError`] of a document that cannot be built.
    fn documents(&self, _settings: &Settings) -> Result<Vec<PublicDocument>, FederationError> {
        Ok(Vec::new())
    }

    /// Returns the table of the binding's onward grant that `credentials`
    /// sets, when one is.
    fn onward_table(&self, _credentials: &Credentials) -> Option<&'static str> {
        None
    }

    /// Resolves the binding's onward grant `credentials` configures under
    /// `section`, when [`Binding::onward_table`] names one.
    ///
    /// # Errors
    ///
    /// The [`Error`] of a grant the binding refuses.
    fn onward(
        &self,
        _section: &str,
        _credentials: &Credentials,
    ) -> Option<Result<Box<dyn OnwardGrant>, Error>> {
        None
    }

    /// Reads the registry from the binding's registry source, blocking the
    /// caller, when it is the source `settings` name.
    fn read_registry(
        &self,
        _settings: &Settings,
    ) -> Option<Result<RegistrySnapshot, FederationError>> {
        None
    }

    /// Holds the sites the binding's services are reached at to the
    /// transport policy, and returns those that travel in cleartext under the
    /// development profile.
    ///
    /// # Errors
    ///
    /// The [`CleartextError`] of the first site the policy refuses.
    fn sites(&self, _settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
        Ok(Vec::new())
    }

    /// Keeps in `fresh` the boot's value of every binding setting a reload
    /// does not apply.
    fn effective(&self, _boot: &Settings, _fresh: &mut Settings) {}

    /// Returns the keys of the binding's settings whose value in `fresh`
    /// differs from the boot's and that take a restart.
    fn needs_restart(&self, _boot: &Settings, _fresh: &Settings) -> Vec<&'static str> {
        Vec::new()
    }

    /// Returns the class a refused reload is logged under, when `error` is
    /// one of the binding's.
    fn class(&self, _error: &FederationError) -> Option<&'static str> {
        None
    }

    /// Whether a record the binding must deliver, such as an audit message,
    /// waits in a spool held in memory, which a restart loses; the startup
    /// banner says so.
    fn spools_in_memory(&self, _settings: &Settings) -> bool {
        false
    }

    /// Logs what the binding is configured to reach, never a value.
    fn log_summary(&self, _settings: &Settings) {}
}

/// Returns the configuration sections a registry reload applies.
#[must_use]
pub fn reloadable() -> Vec<&'static str> {
    CORE_RELOADABLE
        .into_iter()
        .chain(compiled().iter().flat_map(|binding| {
            binding
                .sections()
                .iter()
                .filter(|section| section.reload == Reload::Applies)
                .map(|section| section.key)
        }))
        .collect()
}

/// Returns every role the configured sections of `settings` fill, in wiring
/// order.
#[must_use]
pub fn offers(settings: &Settings) -> Vec<Offer> {
    compiled()
        .iter()
        .flat_map(|binding| binding.offers(settings))
        .collect()
}

/// Refuses `offers` that fill `role` more than once.
///
/// # Errors
///
/// [`RoleConflict`] naming every section that fills `role`, when more than
/// one does.
pub fn single(offers: &[Offer], role: Role) -> Result<Option<Offer>, RoleConflict> {
    let filling: Vec<&Offer> = offers.iter().filter(|offer| offer.role == role).collect();
    match filling.as_slice() {
        [] => Ok(None),
        [offer] => Ok(Some(**offer)),
        _ => Err(RoleConflict {
            role,
            sections: filling.iter().map(|offer| offer.section).collect(),
        }),
    }
}

/// Returns the budgets of the Step-1 services every binding configures in
/// `config`, summed per service.
#[must_use]
pub fn budgets(config: &Config) -> StepBudgets {
    compiled()
        .iter()
        .map(|binding| binding.budgets(config))
        .fold(StepBudgets::default(), |sum, budgets| StepBudgets {
            demographics_ms: sum.demographics_ms.saturating_add(budgets.demographics_ms),
            consent_ms: sum.consent_ms.saturating_add(budgets.consent_ms),
        })
}

/// Returns how a missing localizer names every compiled localizer: the
/// bindings' own first, then the resolvers that localize.
#[must_use]
pub fn localizer_list() -> String {
    let (own, resolving): (Vec<_>, Vec<_>) = compiled()
        .iter()
        .map(|binding| binding.localizers())
        .unzip();
    let all: Vec<&str> = own
        .into_iter()
        .chain(resolving)
        .flatten()
        .copied()
        .collect();
    match all.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}, or {last}", rest.join(", ")),
        Some((last, _)) => (*last).to_owned(),
        None => String::from("no localizer is compiled into this build"),
    }
}

/// Returns how an error that needs a cross-reference resolver names every
/// compiled one, in name order: `[dev] or [pixm]`, or `none in this build`.
#[must_use]
pub fn resolver_list() -> String {
    let mut all: Vec<&str> = compiled()
        .iter()
        .flat_map(|binding| binding.resolvers())
        .copied()
        .collect();
    all.sort_unstable();
    match all.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => (*last).to_owned(),
        None => String::from("none in this build"),
    }
}

/// Builds the resolver of the one binding that offers one over `snapshot`.
///
/// # Errors
///
/// The [`FederationError`] of the resolver.
pub fn resolver(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<ResolverSeam>, FederationError> {
    first(|binding| binding.resolver(settings, snapshot))
}

/// Builds the demographics step of the one binding that offers one,
/// `resolving` saying whether a resolver follows it.
///
/// # Errors
///
/// The [`FederationError`] of the step.
pub fn demographics(
    settings: &Settings,
    resolving: bool,
) -> Result<Option<DemographicsStep>, FederationError> {
    first(|binding| binding.demographics(settings, resolving))
}

/// Builds the localizer of the one binding that offers one of its own over
/// `snapshot`.
///
/// # Errors
///
/// The [`LocalizationError`] of the localizer.
pub fn localizer(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<LocalizerSeam>, LocalizationError> {
    first(|binding| binding.localizer(settings, snapshot))
}

/// Builds the consent pre-filter of the one binding that offers one over
/// `snapshot`.
///
/// # Errors
///
/// The [`FederationError`] of the pre-filter.
pub fn prefilter(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
    first(|binding| binding.prefilter(settings, snapshot))
}

/// Returns the health indicators of what every binding's federation records
/// through.
///
/// # Errors
///
/// The [`FederationError`] of a trail that cannot start.
pub fn indicators(settings: &Settings) -> Result<Vec<Arc<dyn Indicator>>, FederationError> {
    let mut indicators = Vec::new();
    for binding in compiled() {
        indicators.extend(binding.indicators(settings)?);
    }
    Ok(indicators)
}

/// Reads the registry from the binding that is the source `settings` name,
/// when one is.
#[must_use]
pub fn read_registry(settings: &Settings) -> Option<Result<RegistrySnapshot, FederationError>> {
    compiled()
        .iter()
        .find_map(|binding| binding.read_registry(settings))
}

/// Whether a binding is the registry's source in `settings`.
#[must_use]
pub fn sources_registry(settings: &Settings) -> bool {
    offers(settings)
        .iter()
        .any(|offer| offer.role == Role::RegistrySource)
}

/// Returns the class a refused reload over `error` is logged under, when a
/// binding claims it.
#[must_use]
pub fn class(error: &FederationError) -> Option<&'static str> {
    compiled().iter().find_map(|binding| binding.class(error))
}

/// The first role `build` returns from a binding, in wiring order.
fn first<T, E>(
    mut build: impl FnMut(&dyn Binding) -> Result<Option<T>, E>,
) -> Result<Option<T>, E> {
    for binding in compiled() {
        if let Some(built) = build(*binding)? {
            return Ok(Some(built));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        Offer, Reload, Role, RoleConflict, compiled, listed, reloadable, resolver_list, single,
    };

    #[test]
    fn a_role_one_section_may_fill_is_refused_when_two_do() {
        let offers = [
            Offer {
                role: Role::Localizer,
                section: "[xcpd]",
            },
            Offer {
                role: Role::Resolver,
                section: "[pixm]",
            },
            Offer {
                role: Role::Localizer,
                section: "[nl_gf.nvi]",
            },
        ];
        assert_eq!(
            Ok(Some(offers[1])),
            single(&offers, Role::Resolver),
            "one resolver"
        );
        let conflict = single(&offers, Role::Localizer).expect_err("two localizers");
        assert_eq!(
            RoleConflict {
                role: Role::Localizer,
                sections: vec!["[xcpd]", "[nl_gf.nvi]"],
            },
            conflict
        );
        assert_eq!(
            "[xcpd] and [nl_gf.nvi] are both localizers; set one",
            conflict.to_string()
        );
        assert_eq!(Ok(None), single(&offers, Role::ConsentPrefilter));
    }

    #[test]
    fn three_sections_are_listed_in_one_phrase() {
        assert_eq!("[a], [b] and [c]", listed(&["[a]", "[b]", "[c]"]));
        assert_eq!("[a]", listed(&["[a]"]));
    }

    #[test]
    fn an_error_that_needs_a_resolver_names_only_the_compiled_ones() {
        let expected = if cfg!(feature = "binding-ihe") {
            "[dev] or [pixm]"
        } else {
            "[dev]"
        };
        assert_eq!(expected, resolver_list());
    }

    #[test]
    fn every_reloadable_section_belongs_to_one_binding() {
        let reloadable = reloadable();
        assert_eq!(Some(&"registry"), reloadable.first(), "{reloadable:?}");
        for binding in compiled() {
            for section in binding.sections() {
                assert_eq!(
                    section.reload == Reload::Applies,
                    reloadable.contains(&section.key),
                    "{}: {}",
                    binding.name(),
                    section.key
                );
            }
        }
    }
}
