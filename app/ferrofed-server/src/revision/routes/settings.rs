// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The save of deployment-wide settings on the admin listener: `PUT`
//! [`SETTINGS`] writes a revision on the one the operator read with each
//! setting the body names changed, which is applied as any revision.
//!
//! Only a setting the running configuration lists as editable is taken
//! ([`crate::running::catalog`]): one value, never a secret, a list or a
//! table, which an import of the whole configuration changes. Each value is
//! read as an environment override is, TOML syntax or the string it is, and
//! the revision is checked as `config check` checks the file before it is
//! saved. No specification governs this: our own design.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::response::Response;
use ferrofed_registry::running::SettingsRequest;
use http::HeaderMap;

use super::console::{ConsoleError, base_table, created, saved_as};
use super::{Managed, answered, body_refused};
use crate::admin::Operator;
use crate::config::load::env_value;
use crate::error::{self, Code};
use crate::request_id;
use crate::running::{catalog, set};

/// The save of settings.
pub const SETTINGS: &str = "/admin/config/settings";

/// The members of a save's body.
const SETTINGS_SHAPE: &str =
    r#"{"base": <revision>, "reason": "<text>", "settings": {"<dotted key>": "<value>"}}"#;

/// `PUT` [`SETTINGS`]: saves a revision on the base the body names with each
/// of its settings changed, `201` with the revision; it is not applied.
///
/// A body that names no setting, or one that is not an editable setting, is
/// refused `400` (`body-invalid`) naming the key and never its value.
pub(super) async fn save_settings(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(request) = serde_json::from_slice::<SettingsRequest>(&body) else {
        return body_refused(&headers, SETTINGS_SHAPE);
    };
    let request_id = request_id::of(&headers).unwrap_or_default();
    if request.settings.is_empty() {
        return error::response(Code::BodyInvalid, "the body names no setting", request_id);
    }
    if let Some(key) = request
        .settings
        .keys()
        .find(|key| !catalog::entry(key).is_some_and(catalog::Entry::editable))
    {
        return error::response(
            Code::BodyInvalid,
            format!("{key} is not a setting this route changes"),
            request_id,
        );
    }
    let managed = Arc::clone(&managed);
    created(
        answered(&headers, move || {
            let SettingsRequest {
                base,
                reason,
                settings,
            } = request;
            let mut table = base_table(&managed, base)?;
            for (key, raw) in &settings {
                if !set(&mut table, key, env_value(raw)) {
                    return Err(ConsoleError::NotATable(key.clone()));
                }
            }
            saved_as(&managed, (base, &table), (&operator, reason))
        })
        .await,
    )
}
