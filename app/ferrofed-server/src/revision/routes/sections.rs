// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials and binding sections the operator console
//! manages, on the admin listener.
//!
//! Each is owned whole: the configuration file provides it and it is
//! read-only here, refused `409` (`resource-file-owned`), or a revision
//! holds it and the console owns it. A save or a removal writes a new
//! revision on the one the operator read, which is applied as any revision
//! and then awaits its confirmation. A secret is held only as a reference
//! to a file: a value the body sends is written to a new file in
//! `config.secrets_dir`, readable by the gateway's user alone, and never
//! answered, stored, logged or echoed; without that directory a value is
//! refused `409` (`secret-store-unavailable`). No specification governs
//! this: our own design.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::response::Response;
use ferrofed_registry::console::{ConsoleState, Owner};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::resource::{BindingSection, Resource, SectionRequest, SectionResource};
use ferrofed_registry::revision::RevisionId;
use http::HeaderMap;

use super::console::{ConsoleError, base_table, created, saved_as};
use super::{Managed, answered, body_refused, held};
use crate::admin::Operator;
use crate::error::{self, Code};
use crate::request_id;
use crate::revision::resources::{
    SectionFault, compose, inline, remove, set, settings_of, slots, value_at, without_secrets,
    write_secret,
};
use crate::revision::service::{RevisionError, held_table};

/// The save of the onward credentials of one endpoint.
pub const CREDENTIALS: &str = "/admin/config/credentials/{endpoint_id}";

/// The removal of the onward credentials of one endpoint.
pub const CREDENTIALS_REMOVE: &str = "/admin/config/credentials/{endpoint_id}/remove";

/// The save of one binding section.
pub const BINDING: &str = "/admin/config/bindings/{section}";

/// The removal of one binding section.
pub const BINDING_REMOVE: &str = "/admin/config/bindings/{section}/remove";

/// The members of a save's body.
const SECTION_SHAPE: &str = r#"{"base": <revision>, "reason": "<text>", "settings": "<TOML>", "secrets": [{"key": "<key>", "value" | "file": "<text>" | "clear": true}]}"#;

/// The members of a removal's body.
const REMOVE_SHAPE: &str = r#"{"base": <revision>, "reason": "<text>"}"#;

/// The body of a removal.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRequest {
    /// The revision the removal is based on, which must be the newest.
    base: Option<RevisionId>,
    /// Why the change is made.
    reason: String,
}

/// The binding sections this gateway is built with.
#[must_use]
pub fn available() -> Vec<BindingSection> {
    BindingSection::ALL
        .into_iter()
        .filter(|section| {
            let ihe = matches!(
                section,
                BindingSection::Pixm | BindingSection::Pdqm | BindingSection::Xcpd
            );
            let nl = matches!(section, BindingSection::NlGf);
            (!ihe || cfg!(feature = "binding-ihe")) && (!nl || cfg!(feature = "binding-nl"))
        })
        .collect()
}

/// The answer to `fault`: `409` (`secret-store-unavailable`) for a value
/// without `config.secrets_dir`, `500` for a secret file that could not be
/// written, and `400` (`body-invalid`) for the rest, each naming keys alone.
pub(super) fn refused(fault: &SectionFault, request_id: &str) -> Response {
    match fault {
        SectionFault::NoSecretsDir { .. } => {
            error::response(Code::SecretStoreUnavailable, fault.to_string(), request_id)
        }
        SectionFault::Write { .. } => {
            tracing::error!(
                error = crate::chain(fault),
                "a secret the operator sent could not be written to its file"
            );
            error::fixed(Code::Internal, request_id)
        }
        _ => error::response(Code::BodyInvalid, fault.to_string(), request_id),
    }
}

/// Every credential and binding section, the file's and the newest
/// revision's, with its owner, in resource order.
pub(super) fn report(managed: &Managed) -> Result<Vec<SectionResource>, RevisionError> {
    let store = managed.revisions.store();
    let running = store
        .current()?
        .map(|mark| held(&managed.revisions, mark.revision))
        .transpose()?;
    let head = store
        .head()?
        .map(|id| held(&managed.revisions, id))
        .transpose()?;
    let running_table = running
        .as_ref()
        .map(held_table)
        .transpose()?
        .unwrap_or_default();
    let head_table = head
        .as_ref()
        .map(held_table)
        .transpose()?
        .unwrap_or_default();
    let file_owned = managed.revisions.file_owned()?;
    let mut sections = Vec::new();
    for resource in &file_owned {
        sections.push(SectionResource {
            resource: resource.clone(),
            owner: Owner::File,
            state: None,
            settings: None,
            secrets: value_at(&head_table, resource)
                .map(slots)
                .unwrap_or_default(),
        });
    }
    let running_console = running.map(|revision| revision.console).unwrap_or_default();
    let head_console = head.map(|revision| revision.console).unwrap_or_default();
    for resource in &head_console {
        let Some(value) = value_at(&head_table, resource) else {
            continue;
        };
        let runs =
            running_console.contains(resource) && value_at(&running_table, resource) == Some(value);
        sections.push(SectionResource {
            resource: resource.clone(),
            owner: Owner::Console,
            state: Some(if runs {
                ConsoleState::Running
            } else {
                ConsoleState::Pending
            }),
            settings: Some(inline(&without_secrets(value))),
            secrets: slots(value),
        });
    }
    for resource in &running_console {
        if !head_console.contains(resource) && !file_owned.contains(resource) {
            sections.push(SectionResource {
                resource: resource.clone(),
                owner: Owner::Console,
                state: Some(ConsoleState::Removing),
                settings: None,
                secrets: value_at(&running_table, resource)
                    .map(slots)
                    .unwrap_or_default(),
            });
        }
    }
    sections.sort_by(|a, b| a.resource.cmp(&b.resource));
    Ok(sections)
}

/// The credentials of the endpoint `text` names, `None` for text that is
/// no `endpoint_id`.
fn credentials(text: String) -> Option<Resource> {
    EndpointId::new(&text)
        .ok()
        .map(|_valid| Resource::Credentials(text))
}

/// The binding section `text` names, `None` for one this gateway is not
/// built with.
fn binding(text: &str) -> Option<Resource> {
    BindingSection::named(text)
        .filter(|section| available().contains(section))
        .map(Resource::Binding)
}

/// The `404` (`resource-unknown`) of a path that names no resource.
fn unknown(headers: &HeaderMap, what: &str) -> Response {
    error::response(
        Code::ResourceUnknown,
        format!("the path names no {what} this gateway manages"),
        request_id::of(headers).unwrap_or_default(),
    )
}

/// `PUT` [`CREDENTIALS`]: saves a revision with the endpoint's onward
/// credentials in place, `201` with the revision; it is not applied.
pub(super) async fn save_credentials(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(endpoint): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match credentials(endpoint) {
        Some(resource) => save(managed, operator, resource, &headers, &body).await,
        None => unknown(&headers, "endpoint_id"),
    }
}

/// `POST` [`CREDENTIALS_REMOVE`]: saves a revision without the endpoint's
/// onward credentials, `201` with the revision; it is not applied.
pub(super) async fn remove_credentials(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(endpoint): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match credentials(endpoint) {
        Some(resource) => removal(managed, operator, resource, &headers, &body).await,
        None => unknown(&headers, "endpoint_id"),
    }
}

/// `PUT` [`BINDING`]: saves a revision with the binding section in place,
/// `201` with the revision; it is not applied.
pub(super) async fn save_binding(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(section): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match binding(&section) {
        Some(resource) => save(managed, operator, resource, &headers, &body).await,
        None => unknown(&headers, "binding section"),
    }
}

/// `POST` [`BINDING_REMOVE`]: saves a revision without the binding section,
/// `201` with the revision; it is not applied.
pub(super) async fn remove_binding(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(section): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match binding(&section) {
        Some(resource) => removal(managed, operator, resource, &headers, &body).await,
        None => unknown(&headers, "binding section"),
    }
}

/// Saves a revision on the body's base with `resource` as the body gives
/// it, its secret values written to files first and removed again when
/// the save is refused.
async fn save(
    managed: Arc<Managed>,
    operator: Operator,
    resource: Resource,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let Ok(request) = serde_json::from_slice::<SectionRequest>(body) else {
        return body_refused(headers, SECTION_SHAPE);
    };
    created(
        answered(headers, move || {
            let SectionRequest {
                base,
                reason,
                settings,
                secrets,
            } = request;
            if managed.revisions.file_owned()?.contains(&resource) {
                return Err(ConsoleError::ResourceFileOwned(resource));
            }
            let mut table = base_table(&managed, base)?;
            let settings = settings_of(&settings)?;
            let held = value_at(&table, &resource).cloned();
            let dir = managed.revisions.policy().secrets_dir.clone();
            let mut written = Vec::new();
            let composed = compose(settings, held.as_ref(), &secrets, |key, value| {
                let dir = dir.as_deref().ok_or_else(|| SectionFault::NoSecretsDir {
                    key: key.to_owned(),
                })?;
                let path = write_secret(dir, (&resource, key), value)?;
                written.push(path.clone());
                Ok(path)
            });
            let outcome = composed.map_err(ConsoleError::from).and_then(|composed| {
                set(&mut table, &resource, toml::Value::Table(composed));
                saved_as(&managed, (base, &table), (&operator, reason))
            });
            if outcome.is_err() {
                discard(&written);
            }
            outcome
        })
        .await,
    )
}

/// Removes the secret files `written` for a save that was refused.
fn discard(written: &[std::path::PathBuf]) {
    for path in written {
        if let Err(error) = std::fs::remove_file(path) {
            tracing::warn!(
                error = %error,
                "a secret file written for a refused save could not be removed"
            );
        }
    }
}

/// Saves a revision on the body's base without `resource`.
async fn removal(
    managed: Arc<Managed>,
    operator: Operator,
    resource: Resource,
    headers: &HeaderMap,
    body: &[u8],
) -> Response {
    let Ok(request) = serde_json::from_slice::<RemoveRequest>(body) else {
        return body_refused(headers, REMOVE_SHAPE);
    };
    created(
        answered(headers, move || {
            if managed.revisions.file_owned()?.contains(&resource) {
                return Err(ConsoleError::ResourceFileOwned(resource));
            }
            let mut table = base_table(&managed, request.base)?;
            if remove(&mut table, &resource).is_none() {
                return Err(ConsoleError::ResourceUnknown(resource));
            }
            saved_as(
                &managed,
                (request.base, &table),
                (&operator, request.reason),
            )
        })
        .await,
    )
}
