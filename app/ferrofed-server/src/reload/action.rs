// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reload as an admin listener action, `POST /admin/reload`: the reload
//! `SIGHUP` asks for, for a container no signal reaches, such as a
//! distroless image under Kubernetes with no shell to send one.
//!
//! The action answers what the reload changed, or why it was refused, by
//! the class the log names. Each replica reloads its own process, so the
//! answer names `scope: "replica"`. No specification governs reloading: our
//! own design.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use ferrofed_registry::id::{EndpointId, NodeId};
use http::HeaderMap;

use crate::error::{self, Code};
use crate::reload::{Applied, Reloader};
use crate::request_id;

/// What an applied reload answers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Reloaded {
    /// How many members the registry now holds.
    pub members: usize,
    /// The endpoints the new registry adds, in id order.
    pub endpoints_added: Vec<String>,
    /// The endpoints the new registry no longer holds, in id order.
    pub endpoints_removed: Vec<String>,
    /// The members the new registry no longer holds, in id order.
    pub members_removed: Vec<String>,
    /// Whether `[auth]` differs from the one that ran before the reload.
    pub auth_changed: bool,
    /// The listener tables whose certificate files were read again.
    pub certificates: Vec<&'static str>,
    /// The changed settings that take effect only on a restart, by key.
    pub needs_restart: Vec<&'static str>,
    /// Where the reload acted: this process alone.
    pub scope: &'static str,
}

impl From<&Applied> for Reloaded {
    fn from(applied: &Applied) -> Self {
        let named = |ids: &[EndpointId]| ids.iter().map(|id| id.as_str().to_owned()).collect();
        Self {
            members: applied.members,
            endpoints_added: named(&applied.endpoints_added),
            endpoints_removed: named(&applied.endpoints_removed),
            members_removed: applied
                .members_removed
                .iter()
                .map(|id: &NodeId| id.as_str().to_owned())
                .collect(),
            auth_changed: applied.auth_changed,
            certificates: applied.certificates.clone(),
            needs_restart: applied.needs_restart.clone(),
            scope: "replica",
        }
    }
}

/// The message of a reload that carries a body.
const RELOAD_TAKES_NO_BODY: &str =
    "the reload takes no body: it reads the configuration file the process started with";

/// `POST /admin/reload`: reloads as `SIGHUP` does ([`Reloader::reload`])
/// and answers `200` with the [`Reloaded`] it applied.
///
/// The action takes no `body`. A reload refused as a whole answers `422`
/// (`configuration-invalid`), its message naming the refusal's class and
/// cause and never a value of the configuration; the running
/// configuration then stays. A reload that did not finish answers `500`.
pub async fn reload(
    State(reloader): State<Arc<Reloader>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default();
    if !body.is_empty() {
        return error::response(Code::BodyInvalid, RELOAD_TAKES_NO_BODY, request_id);
    }
    // NOTE: no specification governs this: our own design; the reload reads files,
    // so it runs on the blocking pool, as the SIGHUP handler runs it.
    match tokio::task::spawn_blocking(move || reloader.reload()).await {
        Ok(Ok(applied)) => Json(Reloaded::from(&applied)).into_response(),
        Ok(Err(refused)) => error::response(
            Code::ConfigurationInvalid,
            format!(
                "the reload was refused ({}), and the running configuration stays: {}",
                refused.class(),
                crate::chain(&refused)
            ),
            request_id,
        ),
        Err(unfinished) => {
            tracing::error!(
                panicked = unfinished.is_panic(),
                "the reload did not finish; the running configuration stays"
            );
            error::fixed(Code::Internal, request_id)
        }
    }
}
