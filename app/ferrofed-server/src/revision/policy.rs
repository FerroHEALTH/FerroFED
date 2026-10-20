// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a revision that changes a credential or a binding section takes.
//!
//! It takes a confirmation after it applies, a second operator's approval
//! before it applies when `[config] approval` names it, and a directory a
//! secret value the console sends is written to.
//!
//! Every onward credential and every binding section the console manages
//! ([`Resource`]) is a risky resource: a revision that changes one runs
//! awaiting a confirmation, and the gateway rolls it back unless an
//! operator confirms it within `config.confirm_ms`. `config.approval` names
//! resource kinds or single resources a revision may change only once an
//! operator other than its author approved it. No specification governs
//! this: our own design.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use ferrofed_registry::resource::{BindingSection, Resource};

use crate::revision::resources::{present, value_at};

/// A kind of resource `config.approval` may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// Every onward credential, `[credentials."<endpoint id>"]`.
    OnwardCredentials,
    /// The localization failure policy and the localizers: XCPD and the
    /// Dutch Generic Functions' NVI.
    Localization,
    /// Consent disclosure and the consent pre-filter: the Dutch Generic
    /// Functions' Mitz.
    Consent,
    /// Every identity service a binding section configures: PIXm, PDQm,
    /// XCPD and the Dutch Generic Functions.
    IdentityServices,
}

impl Kind {
    /// Every kind, as `config.approval` names it.
    const ALL: [(Self, &'static str); 4] = [
        (Self::OnwardCredentials, "onward-credentials"),
        (Self::Localization, "localization"),
        (Self::Consent, "consent"),
        (Self::IdentityServices, "identity-services"),
    ];

    /// Whether `resource` is of this kind.
    #[must_use]
    pub fn covers(self, resource: &Resource) -> bool {
        use BindingSection::{Consent, Localization, NlGf, Pdqm, Pixm, Xcpd};
        match (self, resource) {
            (Self::OnwardCredentials, Resource::Credentials(_)) => true,
            (Self::Localization, Resource::Binding(section)) => {
                matches!(section, Localization | Xcpd | NlGf)
            }
            (Self::Consent, Resource::Binding(section)) => matches!(section, Consent | NlGf),
            (Self::IdentityServices, Resource::Binding(section)) => {
                matches!(section, Pixm | Pdqm | Xcpd | NlGf)
            }
            _ => false,
        }
    }
}

/// One entry of `config.approval`: a kind of resource, or one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApprovalRule {
    /// Every resource of a kind.
    Kind(Kind),
    /// One resource.
    Resource(Resource),
}

/// An entry of `config.approval` that names no kind and no resource.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "config.approval names \"{0}\", which is no resource kind (onward-credentials, localization, consent, identity-services) and no resource (credentials.<endpoint id>, pixm, pdqm, xcpd, nl_gf, federation.localization, federation.consent)"
)]
pub struct UnknownRule(pub String);

impl FromStr for ApprovalRule {
    type Err = UnknownRule;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some((kind, _)) = Kind::ALL.iter().find(|(_, name)| *name == text) {
            return Ok(Self::Kind(*kind));
        }
        if let Some(endpoint) = text.strip_prefix("credentials.")
            && !endpoint.is_empty()
        {
            return Ok(Self::Resource(Resource::Credentials(endpoint.to_owned())));
        }
        BindingSection::ALL
            .into_iter()
            .find(|section| section.path().join(".") == text)
            .map(|section| Self::Resource(Resource::Binding(section)))
            .ok_or_else(|| UnknownRule(text.to_owned()))
    }
}

impl ApprovalRule {
    /// Whether this rule names `resource`.
    #[must_use]
    pub fn covers(&self, resource: &Resource) -> bool {
        match self {
            Self::Kind(kind) => kind.covers(resource),
            Self::Resource(named) => named == resource,
        }
    }
}

/// What a revision that changes a credential or a binding section takes,
/// read from `[config]`.
#[derive(Clone, PartialEq, Eq)]
pub struct Policy {
    /// How long an applied revision that changed a risky resource awaits
    /// its confirmation, `config.confirm_ms`.
    pub confirm: Duration,
    /// The resources a revision changes only with a second operator's
    /// approval, `config.approval`.
    pub approval: Vec<ApprovalRule>,
    /// The directory a secret value the console sends is written to,
    /// `config.secrets_dir`; without it a value is refused.
    pub secrets_dir: Option<PathBuf>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            confirm: Duration::from_millis(120_000),
            approval: Vec::new(),
            secrets_dir: None,
        }
    }
}

impl fmt::Debug for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Policy")
            .field("confirm", &self.confirm)
            .field("approval", &self.approval)
            .field("secrets_dir", &self.secrets_dir.is_some())
            .finish()
    }
}

impl Policy {
    /// The resources of `changed` that take a second operator's approval.
    #[must_use]
    pub fn needs_approval(&self, changed: &[Resource]) -> Vec<Resource> {
        changed
            .iter()
            .filter(|resource| self.approval.iter().any(|rule| rule.covers(resource)))
            .cloned()
            .collect()
    }
}

/// The resources whose settings differ from `from` to `to`, a resource set
/// in one and not in the other among them, in resource order.
#[must_use]
pub fn changed(from: &toml::Table, to: &toml::Table) -> Vec<Resource> {
    let every: BTreeSet<Resource> = present(from).into_iter().chain(present(to)).collect();
    every
        .into_iter()
        .filter(|resource| value_at(from, resource) != value_at(to, resource))
        .collect()
}

/// `resources` as their tables are written.
#[must_use]
pub fn named(resources: &[Resource]) -> Vec<String> {
    resources.iter().map(ToString::to_string).collect()
}

#[cfg(test)]
mod tests {
    use ferrofed_registry::resource::{BindingSection, Resource};

    use super::{ApprovalRule, Kind, Policy, changed};

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn an_approval_rule_reads_a_kind_or_a_resource() {
        assert_eq!(
            Ok(ApprovalRule::Kind(Kind::OnwardCredentials)),
            "onward-credentials".parse()
        );
        assert_eq!(
            Ok(ApprovalRule::Resource(Resource::Credentials(String::from(
                "node-a.pub"
            )))),
            "credentials.node-a.pub".parse()
        );
        assert_eq!(
            Ok(ApprovalRule::Resource(Resource::Binding(
                BindingSection::Consent
            ))),
            "federation.consent".parse()
        );
        assert!("auth".parse::<ApprovalRule>().is_err());
        assert!("credentials.".parse::<ApprovalRule>().is_err());
    }

    #[test]
    fn the_changed_resources_are_named_whole() {
        let from = table(
            "[credentials.a]\npassword_file = \"/s/a\"\n[credentials.b]\nuser = \"x\"\n\
             [federation]\nbest_effort = true\n[federation.consent]\ndisclose = true\n",
        );
        let to = table(
            "[credentials.a]\npassword_file = \"/s/a2\"\n[credentials.b]\nuser = \"x\"\n\
             [federation]\nbest_effort = false\n[pixm]\ntimeout_ms = 1\n",
        );
        assert_eq!(
            vec![
                Resource::Credentials(String::from("a")),
                Resource::Binding(BindingSection::Pixm),
                Resource::Binding(BindingSection::Consent),
            ],
            changed(&from, &to)
        );
    }

    #[test]
    fn approval_covers_the_kinds_it_names() {
        let policy = Policy {
            approval: vec![ApprovalRule::Kind(Kind::Consent)],
            ..Policy::default()
        };
        let changed = [
            Resource::Credentials(String::from("a")),
            Resource::Binding(BindingSection::NlGf),
            Resource::Binding(BindingSection::Consent),
        ];
        assert_eq!(changed[1..].to_vec(), policy.needs_approval(&changed));
    }
}
