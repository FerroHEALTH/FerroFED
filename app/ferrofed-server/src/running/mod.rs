// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration on the admin listener: `GET` [`RUNNING`],
//! admitted for the issuer's operator scope as every admin read is.
//!
//! The document ([`RunningConfig`]) lists the deployment-wide settings of
//! [`catalog`], each with the value this replica runs, where it comes from
//! (the file, a `FERROFED__` override, the revision it runs, or the
//! default), its default, and whether a change of it waits for the next
//! start; for such a setting it also names the value the next start takes
//! when that differs. Under `config.ui = "off"` the values are the file's
//! and the environment's as the process started and as the last reload
//! applied them; under `managed`, the revision the replica started on and
//! the one it runs. A secret is never a value: the document says whether it
//! is set and names the file it is read from, and nothing here logs a
//! value. No specification governs this: our own design.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ferrofed_registry::revision::RevisionId;
use ferrofed_registry::running::{RunningConfig, SecretSetting, Setting, Source};
use http::HeaderMap;

use crate::config::revisions::Ui;
use crate::config::{ENV_PREFIX, load};
use crate::error::{self, Code};
use crate::reload::Reloader;
use crate::request_id;
use crate::revision::service::{RevisionError, Revisions, held_table};

mod authentication;
pub mod catalog;

use catalog::{ENTRIES, Entry, Kind};

/// The running configuration.
pub const RUNNING: &str = "/admin/config/running";

/// Returns the route of the running configuration over `reloader`, for an
/// operator read gate.
pub fn routes(reloader: Arc<Reloader>) -> Router {
    Router::new()
        .route(RUNNING, get(running))
        .with_state(reloader)
}

/// `GET` [`RUNNING`].
async fn running(State(reloader): State<Arc<Reloader>>, headers: HeaderMap) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    match tokio::task::spawn_blocking(move || document(&reloader)).await {
        Ok(Ok(document)) => Json(document).into_response(),
        Ok(Err(refusal)) => {
            tracing::error!(
                error = crate::chain(&refusal),
                "the running configuration could not be read from the revision store"
            );
            error::fixed(Code::Internal, &request_id)
        }
        Err(failure) => {
            tracing::error!(
                error = %failure,
                "a running configuration request did not complete"
            );
            error::fixed(Code::Internal, &request_id)
        }
    }
}

/// The trees a running configuration is read from.
#[derive(Debug, Clone, Default)]
pub struct Layers {
    /// The tree the process started with.
    pub boot: toml::Table,
    /// The tree the replica runs now, for the settings a reload applies.
    pub now: toml::Table,
    /// The `FERROFED__` overrides of the process, as a tree.
    pub environment: toml::Table,
    /// The file's tree the process started with, which alone sets the
    /// bootstrap core (`[auth]`, `[trust]`) a revision never holds.
    pub file_boot: toml::Table,
    /// The file's tree the replica runs now, as the last reload applied it.
    pub file_now: toml::Table,
    /// The revision the replica runs, under `config.ui = "managed"`.
    pub revision: Option<RevisionId>,
    /// Whether the configuration is managed as revisions.
    pub managed: bool,
}

/// The running configuration of the replica `reloader` reloads.
///
/// # Errors
/// Returns the [`RevisionError`] of a revision store that cannot be read,
/// under `config.ui = "managed"`.
pub fn document(reloader: &Reloader) -> Result<RunningConfig, RevisionError> {
    let environment = environment(std::env::vars());
    let file_boot = reloader.boot_tree().clone();
    let file_now = reloader
        .loaded_tree()
        .unwrap_or_else(|| reloader.boot_tree().clone());
    let layers = match reloader.revisions() {
        None => Layers {
            boot: file_boot.clone(),
            now: file_now.clone(),
            environment,
            file_boot,
            file_now,
            revision: None,
            managed: false,
        },
        Some(revisions) => {
            let running = revisions.replica().running;
            Layers {
                boot: table_of(revisions, reloader.boot_revision())?,
                now: table_of(revisions, running)?,
                environment,
                file_boot,
                file_now,
                revision: running,
                managed: true,
            }
        }
    };
    Ok(of(&layers))
}

/// The configuration revision `id` holds, an empty table for none.
fn table_of(revisions: &Revisions, id: Option<RevisionId>) -> Result<toml::Table, RevisionError> {
    let Some(id) = id else {
        return Ok(toml::Table::new());
    };
    let revision = revisions
        .store()
        .revision(id)?
        .ok_or(RevisionError::Unknown(id))?;
    held_table(&revision)
}

/// The `FERROFED__` overrides among `variables`, as a tree.
pub fn environment(variables: impl IntoIterator<Item = (String, String)>) -> toml::Table {
    let mut tree = toml::Table::new();
    for (name, raw) in variables {
        // NOTE: no specification governs this: our own design; a name that names no
        // key refused the start, so one that does not apply now sets nothing.
        if name.starts_with(ENV_PREFIX) && load::apply_override(&mut tree, &name, &raw).is_err() {
            tracing::debug!(variable = %name, "an environment override names no key");
        }
    }
    tree
}

/// The running configuration `layers` describe.
#[must_use]
pub fn of(layers: &Layers) -> RunningConfig {
    RunningConfig {
        ui: if layers.managed { Ui::Managed } else { Ui::Off }
            .as_str()
            .to_owned(),
        revision: layers.revision,
        settings: ENTRIES
            .iter()
            .map(|entry| setting(entry, layers))
            .chain(authentication::settings(layers))
            .collect(),
    }
}

/// The setting `entry` as `layers` set it.
fn setting(entry: &Entry, layers: &Layers) -> Setting {
    let runs = if entry.restart.is_some() {
        &layers.boot
    } else {
        &layers.now
    };
    let default = entry.default.map(load::env_value);
    if let Kind::Secret { file } = entry.kind {
        let inline = at(runs, entry.key);
        let reference = at(runs, file);
        let source = match reference {
            Some(_) => source(layers, file, reference),
            None => source(layers, entry.key, inline),
        };
        return Setting {
            key: entry.key.to_owned(),
            section: entry.section,
            value: None,
            source,
            default: None,
            restart: entry.restart.is_some(),
            next: None,
            secret: Some(SecretSetting {
                set: inline.is_some() || reference.is_some(),
                reference: reference.and_then(toml::Value::as_str).map(str::to_owned),
            }),
            editable: false,
        };
    }
    let value = at(runs, entry.key);
    let next = entry
        .restart
        .and_then(|_| at(&layers.now, entry.key))
        .filter(|now| Some(*now) != value)
        .cloned();
    Setting {
        key: entry.key.to_owned(),
        section: entry.section,
        source: source(layers, entry.key, value),
        value: value.cloned().or_else(|| default.clone()),
        default,
        restart: entry.restart.is_some(),
        next,
        secret: None,
        editable: layers.managed && entry.editable(),
    }
}

/// Where `value`, the value of `key`, comes from.
fn source(layers: &Layers, key: &str, value: Option<&toml::Value>) -> Source {
    match value {
        None => Source::Default,
        Some(value) if at(&layers.environment, key) == Some(value) => Source::Environment,
        Some(_) if layers.managed => Source::Revision,
        Some(_) => Source::File,
    }
}

/// The value at the dotted `key` of `tree`.
pub(crate) fn at<'a>(tree: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut segments = key.split('.');
    let first = segments.next()?;
    segments.try_fold(tree.get(first)?, |value, segment| {
        value.as_table().and_then(|table| table.get(segment))
    })
}

/// Sets the dotted `key` of `tree` to `value`, creating each parent table;
/// `false` when a parent is set to something other than a table.
pub(crate) fn set(tree: &mut toml::Table, key: &str, value: toml::Value) -> bool {
    let segments: Vec<&str> = key.split('.').collect();
    let Some((last, parents)) = segments.split_last() else {
        return false;
    };
    let mut cursor = tree;
    for parent in parents {
        let entry = cursor
            .entry((*parent).to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(next) = entry.as_table_mut() else {
            return false;
        };
        cursor = next;
    }
    cursor.insert((*last).to_owned(), value);
    true
}

#[cfg(test)]
mod tests;
