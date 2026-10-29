// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the configuration page renders and what its forms post.
//!
//! The page is drawn in both halves: the header with the revisions it names,
//! the card of the revision saved and not applied, the tabs, and the tab's
//! own cards with their forms. Its forms post to the server alone, as
//! structured fields:
//! each endpoint, setting and secret a row of its own, never one line of
//! separated text. A form's `Debug` output names which fields are filled,
//! never what they hold: a secret's value among them, which the page never
//! renders.

use std::fmt;

use ferrohealth_secret::secret::WriteOnly;
use serde::{Deserialize, Serialize};

/// Which part of the configuration page a path names: the tab, the item
/// inside it, and the revision a comparison names.
///
/// It crosses to the server as the configuration's load names it, and the
/// server reads it; a part it does not know is refused there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Screen {
    /// The tab, as its path segment names it; empty for the first tab.
    pub tab: String,
    /// The item inside the tab, such as `new` or a member's `node_id`.
    pub item: String,
    /// The revision the history compares with the running one.
    pub compare: String,
}

/// The configuration page as it is drawn, in the parts the page
/// places around the answer of a form.
#[derive(Debug, Default)]
pub struct ConfigView {
    /// The page header: its title, its subtitle and the revision pills.
    pub header: Option<leptos::prelude::AnyView>,
    /// The card of the revision saved and not applied, and of what the
    /// running revision awaits.
    pub pending: Option<leptos::prelude::AnyView>,
    /// The tab shown, which the binder tabs mark.
    pub tab: Option<crate::configure::render::Tab>,
    /// The cards above the tab's main card.
    pub lead: Option<leptos::prelude::AnyView>,
    /// The tab's main card.
    pub main: Option<leptos::prelude::AnyView>,
    /// Whether configuration revisions are off, so only the settings tabs
    /// show, read-only.
    pub file_only: bool,
}

/// One Query API endpoint row of the member form.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryEndpointInput {
    /// The row's key on the page, which names its fields.
    pub key: String,
    /// The `endpoint_id`.
    pub id: String,
    /// The base URL.
    pub url: String,
    /// Whether the endpoint is in service; out of service it is
    /// `suspended`.
    pub in_service: bool,
}

/// One FHIR endpoint row of the member form.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FhirEndpointInput {
    /// The row's key on the page, which names its fields.
    pub key: String,
    /// The `endpoint_id`.
    pub id: String,
    /// The base URL.
    pub url: String,
    /// The identifier system the endpoint is told the `ehr_id` in.
    pub system: String,
    /// The categories it serves, each a code.
    pub categories: Vec<String>,
    /// The profiles it serves, each a canonical URL.
    pub profiles: Vec<String>,
    /// Whether the endpoint is in service.
    pub in_service: bool,
}

/// One setting of a credential or a binding section: its dotted key and
/// its value, or its values for a list.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingInput {
    /// The row's key on the page, which names its fields.
    pub row: String,
    /// The dotted key inside the resource, such as `oauth2.client_id`.
    pub key: String,
    /// The value as the operator entered it.
    pub value: String,
    /// The values of a list, each entered on its own.
    pub values: Vec<String>,
    /// The value the form was drawn with, for a form that sends only what
    /// the operator changed.
    pub was: String,
}

/// What a form does with one secret of a resource.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretInput {
    /// The row's key on the page, which names its fields.
    pub row: String,
    /// The dotted key inside the resource, such as `oauth2.client_secret`.
    pub key: String,
    /// The choice: `keep`, `set`, `file` or `clear`.
    pub choice: String,
    /// The secret's value for `set`, which the gateway writes to a file of
    /// its own; zeroed when dropped, and never rendered or logged.
    pub value: WriteOnly,
    /// The file the gateway reads the secret from, for `file`.
    pub file: String,
}

/// A configuration form as the browser posts it; each form sends its own
/// fields and names its action.
///
/// Every field is what the operator entered, read on the server; a field a
/// form does not send is empty.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigForm {
    /// The action: `add-member`, `save-member`, `remove-member`, `apply`,
    /// `rollback`, `store-query`, `save-credentials`, `save-binding`,
    /// `remove-section`, `confirm`, `approve` or `import`.
    pub action: String,
    /// The revision a change is based on.
    pub base: String,
    /// The revision an apply names.
    pub revision: String,
    /// The running revision a rollback names.
    pub current: String,
    /// Why the change is made.
    pub reason: String,
    /// Not empty once the operator took the confirmation step.
    pub confirm: String,
    /// The member's organisation id.
    pub organisation: String,
    /// The organisation's display name.
    pub organisation_name: String,
    /// The member's `node_id`.
    pub node: String,
    /// The node's `system_id`.
    pub system_id: String,
    /// The node's CDR product.
    pub product: String,
    /// The product's version.
    pub product_version: String,
    /// The node's Query API endpoints, one row each.
    pub query_endpoints: Vec<QueryEndpointInput>,
    /// The node's FHIR endpoints, one row each.
    pub fhir_endpoints: Vec<FhirEndpointInput>,
    /// Not empty when the form showed the FHIR endpoints; a form without
    /// them keeps the held FHIR endpoints whole.
    pub fhir_shown: String,
    /// The stored query's qualified name.
    pub name: String,
    /// The stored query's version.
    pub version: String,
    /// The stored query's AQL.
    pub aql: String,
    /// The endpoint whose onward credentials a form saves.
    pub endpoint: String,
    /// The binding section a form saves, as its route names it.
    pub section: String,
    /// The credential or binding section a removal names, as
    /// `credentials:<endpoint id>` or `binding:<section>`.
    pub resource: String,
    /// The grant a credential uses: `transport`, `bearer`, `basic`,
    /// `oauth2`, `fapi2` or `nuts`.
    pub grant: String,
    /// The settings of a credential or a binding section, less its
    /// secrets, one row each.
    pub settings: Vec<SettingInput>,
    /// What the form does with each secret it shows.
    pub secrets: Vec<SecretInput>,
    /// The TOML an import saves as a dry-run revision.
    pub toml: String,
    /// The row an Add or a Remove button changes, which runs nothing
    /// (`crate::configure::draft::Edit`).
    pub edit: String,
}

impl ConfigForm {
    /// The form with every secret's value emptied, as it comes back to the
    /// page to be drawn again: a secret is never drawn again.
    #[must_use]
    pub fn without_secrets(mut self) -> Self {
        for secret in &mut self.secrets {
            secret.value = WriteOnly::new(String::new());
        }
        self
    }

    /// The form the posted `fields` hold, by the field names the forms
    /// post: each row's fields named by its key.
    #[must_use]
    pub fn of(fields: &[(String, String)]) -> Self {
        let one = |name: &str| {
            fields
                .iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.replace("\r\n", "\n"))
                .unwrap_or_default()
        };
        let all = |name: &str| -> Vec<String> {
            fields
                .iter()
                .filter(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
                .collect()
        };
        let field = |name: &str| one(&format!("form[{name}]"));
        let query_endpoints = all("form[query_rows]")
            .into_iter()
            .map(|key| {
                let part = |part: &str| one(&format!("form[query.{key}.{part}]"));
                QueryEndpointInput {
                    id: part("id"),
                    url: part("url"),
                    in_service: !part("in_service").is_empty(),
                    key,
                }
            })
            .collect();
        let fhir_endpoints = all("form[fhir_rows]")
            .into_iter()
            .map(|key| {
                let part = |part: &str| one(&format!("form[fhir.{key}.{part}]"));
                let many = |part: &str| all(&format!("form[fhir.{key}.{part}]"));
                FhirEndpointInput {
                    id: part("id"),
                    url: part("url"),
                    system: part("system"),
                    categories: many("categories"),
                    profiles: many("profiles"),
                    in_service: !part("in_service").is_empty(),
                    key,
                }
            })
            .collect();
        let settings = all("form[setting_rows]")
            .into_iter()
            .map(|row| {
                let part = |part: &str| format!("form[setting.{row}.{part}]");
                SettingInput {
                    key: one(&part("key")),
                    value: one(&part("value")),
                    values: all(&part("values")),
                    was: one(&part("was")),
                    row,
                }
            })
            .collect();
        let secrets = all("form[secret_rows]")
            .into_iter()
            .map(|row| {
                let part = |part: &str| one(&format!("form[secret.{row}.{part}]"));
                SecretInput {
                    key: part("key"),
                    choice: part("choice"),
                    value: WriteOnly::new(part("value")),
                    file: part("file"),
                    row,
                }
            })
            .collect();
        Self {
            action: field("action"),
            base: field("base"),
            revision: field("revision"),
            current: field("current"),
            reason: field("reason"),
            confirm: field("confirm"),
            organisation: field("organisation"),
            organisation_name: field("organisation_name"),
            node: field("node"),
            system_id: field("system_id"),
            product: field("product"),
            product_version: field("product_version"),
            query_endpoints,
            fhir_endpoints,
            fhir_shown: field("fhir_shown"),
            name: field("name"),
            version: field("version"),
            aql: field("aql"),
            endpoint: field("endpoint"),
            section: field("section"),
            resource: field("resource"),
            grant: field("grant"),
            settings,
            secrets,
            toml: field("toml"),
            edit: field("edit"),
        }
    }
}

impl fmt::Debug for QueryEndpointInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryEndpointInput")
            .field("key", &self.key)
            .field("in_service", &self.in_service)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for FhirEndpointInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FhirEndpointInput")
            .field("key", &self.key)
            .field("categories", &self.categories.len())
            .field("profiles", &self.profiles.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for SettingInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingInput")
            .field("key", &self.key)
            .field("value", &!self.value.trim().is_empty())
            .field("values", &self.values.len())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretInput")
            .field("choice", &self.choice)
            .field("value", &self.value)
            .field("file", &!self.file.trim().is_empty())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for ConfigForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = |text: &str| !text.trim().is_empty();
        f.debug_struct("ConfigForm")
            .field("action", &self.action)
            .field("reason", &filled(&self.reason))
            .field("confirm", &filled(&self.confirm))
            .field("node", &filled(&self.node))
            .field("query_endpoints", &self.query_endpoints.len())
            .field("fhir_endpoints", &self.fhir_endpoints.len())
            .field("fhir_shown", &filled(&self.fhir_shown))
            .field("name", &filled(&self.name))
            .field("aql", &filled(&self.aql))
            .field("grant", &self.grant)
            .field("settings", &self.settings.len())
            .field("secrets", &self.secrets)
            .field("toml", &filled(&self.toml))
            .field("edit", &self.edit)
            .finish_non_exhaustive()
    }
}
