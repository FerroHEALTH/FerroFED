// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running configuration on the admin listener: `GET` [`RUNNING`],
//! admitted for the issuer's operator scope as every admin read is.
//!
//! The document ([`RunningConfig`]) lists the deployment-wide settings of
//! [`catalog`], each with the value this replica runs, where it comes from
//! (the file, a `FERROFED__` override, a `--set` pair, the revision it
//! runs, or the default, as the loader's provenance names it), its default, and whether a change of it waits for the next
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
use ferrohealth_config::provenance::{Provenance, Source as Origin};
use http::HeaderMap;

use crate::config::revisions::Ui;
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
    /// Where each key of [`Layers::file_boot`] came from: the file, a
    /// `FERROFED__` override or a `--set` pair.
    pub provenance: Provenance,
    /// The file's tree the process started with, its overrides applied,
    /// which alone sets the bootstrap core (`[auth]`, `[trust]`) a revision
    /// never holds.
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
    let provenance = reloader.provenance().clone();
    let file_boot = reloader.boot_tree().clone();
    let file_now = reloader
        .loaded_tree()
        .unwrap_or_else(|| reloader.boot_tree().clone());
    let layers = match reloader.revisions() {
        None => Layers {
            boot: file_boot.clone(),
            now: file_now.clone(),
            provenance,
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
                provenance,
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
    let default = entry.default.map(catalog::default_value);
    if let Kind::Secret { file } = entry.kind {
        let inline = at(runs, entry.key);
        let reference = at(runs, file);
        let source = match reference {
            Some(_) => secret_source(layers, file, reference),
            None => secret_source(layers, entry.key, inline),
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
    sourced(
        layers,
        key,
        value,
        layers.provenance.source(&provenance_key(key)),
    )
}

/// Where `value`, the value of the secret `key` or of its `_file` sibling,
/// comes from, counting the two keys as one setting.
fn secret_source(layers: &Layers, key: &str, value: Option<&toml::Value>) -> Source {
    sourced(
        layers,
        key,
        value,
        layers.provenance.secret_source(&provenance_key(key)),
    )
}

/// Where `value`, the value of `key`, comes from, when the process's own
/// sources set `key` from `origin`.
///
/// An override names the source only while what runs is what it set: under
/// `managed`, a revision saved since may hold another value.
pub(crate) fn sourced(
    layers: &Layers,
    key: &str,
    value: Option<&toml::Value>,
    origin: Origin,
) -> Source {
    let overridden = match origin {
        Origin::Environment => Some(Source::Environment),
        Origin::Set => Some(Source::Set),
        Origin::Default | Origin::File => None,
    };
    match (value, overridden) {
        (None, _) => Source::Default,
        (Some(value), Some(overridden)) if indexed(&layers.file_boot, key) == Some(value) => {
            overridden
        }
        (Some(_), _) if layers.managed => Source::Revision,
        (Some(_), _) => Source::File,
    }
}

/// The dotted key the loader's provenance knows `key` by: an element of an
/// array, `trust.issuer[0].issuer`, is set by the array, `trust.issuer`.
pub(crate) fn provenance_key(key: &str) -> String {
    key.split_once('[')
        .map_or(key, |(array, _)| array)
        .to_owned()
}

/// The value at `key` of `tree`, a dotted key whose segments may name an
/// array element as `name[index]`.
pub(crate) fn indexed<'a>(tree: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut value: Option<&toml::Value> = None;
    for segment in key.split('.') {
        let (name, index) = match segment.split_once('[') {
            Some((name, rest)) => (name, rest.strip_suffix(']')?.parse::<usize>().ok()),
            None => (segment, None),
        };
        let table = match value {
            None => tree,
            Some(held) => held.as_table()?,
        };
        let held = table.get(name)?;
        value = Some(match index {
            Some(index) => held.as_array()?.get(index)?,
            None => held,
        });
    }
    value
}

/// The value at the dotted `key` of `tree`.
pub(crate) fn at<'a>(tree: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut segments = key.split('.');
    let first = segments.next()?;
    segments.try_fold(tree.get(first)?, |value, segment| {
        value.as_table().and_then(|table| table.get(segment))
    })
}

#[cfg(test)]
mod tests;
