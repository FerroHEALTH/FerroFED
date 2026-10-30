// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The actions the operator posts: the clear of a held `ehr_id`, the
//! stored-query drift repair, a query, and the changes of the
//! configuration.
//!
//! Each action is a server function behind FerroUI's `<ActionForm>` in its
//! answer mode. Once the page has hydrated, the form calls the function and
//! the page draws its answer in place. With no WebAssembly the form posts
//! to the function, and `ferrohealth_ui::widget::action::draw_answers`
//! draws the page the form was on with the answer. Either way the function
//! checks the operator's session itself, reads the form on the server, asks
//! the gateway as the operator, and the answer never reaches a URL, the
//! history or a log; a refusal stays with the fields it is about, as
//! FerroUI asks. Every unsafe request passes the console's same-origin
//! check before it reaches a function (`ferrohealth_server::same_origin`). No
//! specification governs the console's actions: our own design.

pub mod clear;
pub mod configure;
pub mod fields;
pub mod query;
pub mod repair;

use serde::{Deserialize, Serialize};
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, StatusCode};
use crate::gateway::GatewayError;
use crate::read::{ActionKind, Asked, Stop, stopped};
use crate::read::{Fault, Refusal};
use crate::server::ConsoleState;

/// What one replica did with an action: its report, or why it gave none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acted<T> {
    /// The replica ran the action and reported what it did.
    Done(T),
    /// The replica refused the action, with its status and stable code.
    Refused(Refusal),
    /// The console could not reach the replica or read its answer.
    Fault(Fault),
}

/// One replica's answer to an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicaAnswer<T> {
    /// The replica's name.
    pub replica: String,
    /// What it did.
    pub acted: Acted<T>,
    /// The gateway's message of a `configuration-invalid` refusal: the
    /// errors `config check` reports, which name each key refused.
    pub said: Option<String>,
}

/// The stable code of a configuration the gateway refuses as `config
/// check` does, whose message the page shows.
pub const CONFIGURATION_INVALID: &str = "configuration-invalid";

/// What `replica` did with the action `what`, `answered` read by `read`,
/// logged by replica, status and code, never by what the action named.
pub fn acted<A, T>(
    kind: ActionKind,
    (replica, what): (&str, &str),
    answered: Result<A, GatewayError>,
    read: impl FnOnce(A) -> T,
) -> ReplicaAnswer<T> {
    let mut said = None;
    let acted = match answered {
        Ok(report) => {
            tracing::info!(replica, action = what, "an operator's {} ran", kind.noun());
            Acted::Done(read(report))
        }
        Err(error) => match stopped(
            &error,
            Asked::Action {
                kind,
                what,
                replica,
            },
        ) {
            Stop::Refused(refusal) => {
                if let Refusal::Gateway {
                    code: Some(code), ..
                } = &refusal
                    && code == CONFIGURATION_INVALID
                {
                    said = error.message();
                }
                Acted::Refused(refusal)
            }
            Stop::Fault(fault) => Acted::Fault(fault),
        },
    };
    ReplicaAnswer {
        replica: replica.to_owned(),
        acted,
        said,
    }
}

/// The value of the field `name` of a posted form, the last one posted,
/// trimmed, or `None` when it is missing or empty.
#[must_use]
pub fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .rev()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

/// Every value of the field `name` of a posted form, in the order posted.
#[must_use]
pub fn fields_named<'a>(fields: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    fields
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .collect()
}

/// Serves the gateway's `OPTIONS {base}/` document as a download, read as
/// the operator of the request: About's Download. A refusal answers with
/// the gateway's status, never an empty file.
pub async fn options_document(
    axum::Extension(state): axum::Extension<ConsoleState>,
    headers: HeaderMap,
) -> Response {
    let Ok(token) = crate::read::signed_in_with(&state, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.gateway().self_description(&token).await {
        Ok(description) => match serde_json::to_string_pretty(&description) {
            Ok(text) => (
                [
                    (
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    ),
                    (
                        http::header::CONTENT_DISPOSITION,
                        http::HeaderValue::from_static("attachment; filename=\"options.json\""),
                    ),
                ],
                text,
            )
                .into_response(),
            Err(_unwritable) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        },
        Err(error) => match stopped(&error, Asked::View) {
            Stop::Refused(Refusal::Gateway { status, .. }) => StatusCode::from_u16(status)
                .unwrap_or(StatusCode::BAD_GATEWAY)
                .into_response(),
            Stop::Refused(Refusal::NotAuthenticated { .. }) => {
                StatusCode::UNAUTHORIZED.into_response()
            }
            Stop::Refused(Refusal::Invalid { .. }) | Stop::Fault(_) => {
                StatusCode::BAD_GATEWAY.into_response()
            }
        },
    }
}
