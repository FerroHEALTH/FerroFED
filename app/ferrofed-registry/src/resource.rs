// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials and binding sections the operator console manages, each
//! owned whole, and the bodies that change one.
//!
//! A [`Resource`] is one onward credential (`[credentials."<endpoint id>"]`)
//! or one binding section ([`BindingSection`]): the file provides it and it
//! is read-only in the console, or a configuration revision holds it and
//! the console owns it. Its secrets are written only as references to
//! files: [`SecretSlot`] names whether one is set and where, and a
//! [`SecretChange`] sets one by a value the gateway writes to a file of its
//! own, by a reference, or clears it. No specification governs this: our
//! own design.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::console::{ConsoleState, Owner};
use crate::revision::RevisionId;
use crate::secret::WriteOnly;

/// A binding section the console manages whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BindingSection {
    /// The PIXm resolver, `[pixm]`.
    Pixm,
    /// The PDQm demographics step, `[pdqm]`.
    Pdqm,
    /// The XCPD localizer, `[xcpd]`.
    Xcpd,
    /// The Dutch Generic Functions, `[nl_gf]`: the NVI localizer and the
    /// Mitz consent pre-filter.
    NlGf,
    /// The localizer's failure policy and budget,
    /// `[federation.localization]` (§14.1).
    Localization,
    /// How a member the consent pre-filter excludes is reported,
    /// `[federation.consent]` (N27a).
    Consent,
}

impl BindingSection {
    /// Every binding section, in the order a page lists them.
    pub const ALL: [Self; 6] = [
        Self::Pixm,
        Self::Pdqm,
        Self::Xcpd,
        Self::NlGf,
        Self::Localization,
        Self::Consent,
    ];

    /// The section as its route and `config.approval` name it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pixm => "pixm",
            Self::Pdqm => "pdqm",
            Self::Xcpd => "xcpd",
            Self::NlGf => "nl_gf",
            Self::Localization => "localization",
            Self::Consent => "consent",
        }
    }

    /// The section named `name`, as [`BindingSection::as_str`] writes it.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.as_str() == name)
    }

    /// The keys of the section's table in the configuration, outermost
    /// first.
    #[must_use]
    pub const fn path(self) -> &'static [&'static str] {
        match self {
            Self::Pixm => &["pixm"],
            Self::Pdqm => &["pdqm"],
            Self::Xcpd => &["xcpd"],
            Self::NlGf => &["nl_gf"],
            Self::Localization => &["federation", "localization"],
            Self::Consent => &["federation", "consent"],
        }
    }
}

/// One whole resource the console manages beside the members.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Resource {
    /// The onward credentials of one endpoint,
    /// `[credentials."<endpoint id>"]`.
    Credentials(String),
    /// One binding section.
    Binding(BindingSection),
}

impl Resource {
    /// The keys of the resource's table in the configuration, outermost
    /// first.
    #[must_use]
    pub fn path(&self) -> Vec<&str> {
        match self {
            Self::Credentials(endpoint) => vec!["credentials", endpoint.as_str()],
            Self::Binding(section) => section.path().to_vec(),
        }
    }
}

impl fmt::Display for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(endpoint) => {
                write!(f, "[credentials.{}]", toml::Value::from(endpoint.as_str()))
            }
            Self::Binding(section) => write!(f, "[{}]", section.path().join(".")),
        }
    }
}

/// The keys whose value is a secret with a `_file` sibling, wherever they
/// sit in the configuration.
///
/// The gateway refuses an inline value of one in a revision and keeps its
/// `_file` reference, and the console asks for one through a password field
/// and never shows it as a setting. The gateway's tests fail on a secret it
/// reads through `_file` that is missing here.
pub const SECRETS: [&str; 4] = ["password", "client_identity", "client_secret", "assertion"];

/// One secret of a resource: its key inside the resource, and the file it
/// is read from when it is set. Never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretSlot {
    /// The dotted key inside the resource, such as `oauth2.client_secret`.
    pub key: String,
    /// The file the gateway reads the secret from, its `_file` reference.
    pub reference: String,
}

/// One credential or binding section, with its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionResource {
    /// The resource.
    pub resource: Resource,
    /// Who provides it.
    pub owner: Owner,
    /// Where it stands, for one the console holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ConsoleState>,
    /// Its settings less its secrets, as one TOML inline table, for one the
    /// console holds: what an edit starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<String>,
    /// Its secrets that are set, each by reference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretSlot>,
}

/// A change to one secret of a resource: exactly one of a value, a
/// reference or a clear.
///
/// `Debug` never shows the value ([`WriteOnly`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretChange {
    /// The dotted key inside the resource, such as `oauth2.client_secret`.
    pub key: String,
    /// The secret's value, which the gateway writes to a new file in its
    /// `config.secrets_dir` and references; it is never stored, echoed or
    /// logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<WriteOnly>,
    /// A file the gateway reads the secret from, which it references as it
    /// is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Whether the secret is removed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear: bool,
}

/// The body of the save of a credential or a binding section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionRequest {
    /// The revision the save is based on, which must be the newest.
    pub base: Option<RevisionId>,
    /// Why the change is made.
    pub reason: String,
    /// The resource's settings less its secrets, as TOML: the table's own
    /// keys, or one inline table.
    pub settings: String,
    /// The changes to its secrets; a secret not named keeps its reference.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretChange>,
}

#[cfg(test)]
mod tests {
    use super::{BindingSection, Resource, SecretChange};
    use crate::secret::WriteOnly;

    #[test]
    fn a_resource_names_its_table() {
        assert_eq!(
            "[credentials.\"node-a.pub\"]",
            Resource::Credentials(String::from("node-a.pub")).to_string()
        );
        assert_eq!(
            "[federation.consent]",
            Resource::Binding(BindingSection::Consent).to_string()
        );
        assert_eq!(Some(BindingSection::NlGf), BindingSection::named("nl_gf"));
        assert_eq!(None, BindingSection::named("auth"));
    }

    #[test]
    fn a_secret_change_shows_no_value_in_debug() {
        let change = SecretChange {
            key: String::from("password"),
            value: Some(WriteOnly::new("synthetic-secret-value-7")),
            file: None,
            clear: false,
        };
        assert!(!format!("{change:?}").contains("synthetic-secret-value-7"));
    }
}
