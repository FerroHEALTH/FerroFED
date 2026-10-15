// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the configuration page renders and what its forms post.
//!
//! The page is rendered on the server and crosses to the browser as HTML:
//! the revisions, the members and stored queries with their owners, the
//! history, and each form's fields. Its forms cross to the server in a
//! `POST` body alone, and a form's `Debug` output names which fields are
//! filled, never what they hold: a secret's value among them, which the
//! page never renders.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The configuration page as the server rendered it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigView {
    /// The read-only part, rendered as HTML.
    pub html: String,
    /// The forms the operator may submit, each rendered whole as HTML.
    pub forms: String,
}

/// A configuration form as the browser posts it; each form sends its own
/// fields and names its action.
///
/// Every field is the text the operator entered, read on the server; a
/// checkbox left clear and a field a form does not send are empty.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigForm {
    /// The action: `save-member`, `remove-member`, `apply`, `rollback`,
    /// `store-query`, `save-credentials`, `save-binding`, `remove-section`,
    /// `confirm` or `approve`.
    pub action: String,
    /// The revision a change is based on.
    pub base: String,
    /// The revision an apply names.
    pub revision: String,
    /// The running revision a rollback names.
    pub current: String,
    /// Why the change is made.
    pub reason: String,
    /// Not empty when the operator ticked the confirmation.
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
    /// The node's Query API endpoints: `endpoint_id`, base URL and
    /// optionally `suspended`, separated by spaces, each endpoint separated
    /// by a semicolon.
    pub endpoints: String,
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
    /// The settings of a credential or a binding section less its secrets,
    /// as one TOML inline table.
    pub settings: String,
    /// The dotted key of the secret a form sets or clears.
    pub secret_key: String,
    /// The secret's value, which the gateway writes to a file of its own;
    /// never rendered or logged.
    pub secret_value: String,
    /// The file the gateway reads the secret from.
    pub secret_file: String,
    /// Not empty when the operator clears the secret.
    pub secret_clear: String,
}

impl fmt::Debug for ConfigForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = |text: &str| !text.trim().is_empty();
        f.debug_struct("ConfigForm")
            .field("action", &self.action)
            .field("reason", &filled(&self.reason))
            .field("confirm", &filled(&self.confirm))
            .field("node", &filled(&self.node))
            .field("endpoints", &filled(&self.endpoints))
            .field("name", &filled(&self.name))
            .field("aql", &filled(&self.aql))
            .field("settings", &filled(&self.settings))
            .field("secret_key", &filled(&self.secret_key))
            .field("secret_value", &filled(&self.secret_value))
            .field("secret_file", &filled(&self.secret_file))
            .finish_non_exhaustive()
    }
}
