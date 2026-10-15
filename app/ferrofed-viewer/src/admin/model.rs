// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the admin action forms post, and what the console renders of each
//! replica's answer.
//!
//! The form crosses to the server in a `POST` body alone, and its `Debug`
//! output names which fields are filled, never what they hold. The answer
//! is rendered on the server and crosses to the browser as HTML; the
//! per-replica outcomes it is rendered from never leave the server.

use std::fmt;

use serde::{Deserialize, Serialize};

/// An admin action form, as the browser posts it: the collision clear's
/// fields and the drift repair's, each form sending its own.
///
/// Every field is the text the operator entered, read on the server; a
/// checkbox left clear and a field a form does not send are empty.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdminForm {
    /// The replica to act on by name, or `*` for every replica.
    pub replica: String,
    /// The `ehr_id` to clear.
    pub ehr_id: String,
    /// The qualified name of the stored query to send again.
    pub name: String,
    /// Its version.
    pub version: String,
    /// The members to send it to: `*`, or `endpoint_id`s separated by
    /// commas.
    pub endpoints: String,
    /// Not empty when the operator ticked the confirmation.
    pub confirm: String,
}

impl fmt::Debug for AdminForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = |text: &str| !text.trim().is_empty();
        f.debug_struct("AdminForm")
            .field("replica", &filled(&self.replica))
            .field("ehr_id", &filled(&self.ehr_id))
            .field("name", &filled(&self.name))
            .field("version", &filled(&self.version))
            .field("endpoints", &filled(&self.endpoints))
            .field("confirm", &filled(&self.confirm))
            .finish()
    }
}

/// An admin action's answer as the server rendered it for the page: what
/// each replica did, or why no replica was asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionAnswer {
    /// The answer rendered as HTML, naming every replica it went to.
    pub html: String,
}
