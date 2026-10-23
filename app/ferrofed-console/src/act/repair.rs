// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored-query drift repair: one version of a stored query sent again
//! to the members chosen (§12.7, stored-query-drift).
//!
//! The form on the stored query's page posts the version and the members,
//! `*` for every active member or the endpoints chosen, and the first
//! replica configured sends it, since the stored-query store is the
//! replicas' shared one. The page answers with each member's outcome as a
//! fan-out (§9.5).

use axum::Extension;
use axum::extract::{Form, Path};
use axum::response::Response;
use http::{HeaderMap, HeaderValue, Uri};
use leptos::context::provide_context;

use crate::act::query::EndpointLine;
use crate::act::{ReplicaAnswer, acted, field, render};
use crate::gateway::admin::{Distribution, Redistribution};
use crate::locale::{Msg, tr_in};
use crate::read::Refusal;
use crate::server::ConsoleState;

/// What a replica reports of a drift repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairLine {
    /// The status the replica answered with.
    pub status: u16,
    /// Whether every member took the version (§11.4, N37).
    pub complete: bool,
    /// The version sent.
    pub version: String,
    /// What the registry did with it.
    pub registry: Option<String>,
    /// Each member's outcome.
    pub endpoints: Vec<EndpointLine>,
}

/// The outcome of a repair the page draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairOutcome {
    /// The replica asked, with its answer.
    Answered(ReplicaAnswer<RepairLine>),
    /// The form cannot be sent as it stands.
    Refused(Refusal),
}

/// The line of `distribution`.
fn line(distribution: Distribution) -> RepairLine {
    let federation = &distribution.federation;
    RepairLine {
        status: distribution.status.as_u16(),
        complete: federation.complete(),
        endpoints: federation
            .endpoints()
            .iter()
            .map(|endpoint| EndpointLine {
                id: endpoint.id().as_str().to_owned(),
                status: endpoint.status(),
                latency_ms: endpoint.outcome().latency_ms(),
                row_count: endpoint.row_count(),
                error: endpoint.outcome().error().map(|error| {
                    crate::act::query::error_text(&ferrohealth_ui::i18n::Locale::english(), error)
                }),
            })
            .collect(),
        version: distribution.definition.version.clone(),
        registry: distribution.registry,
    }
}

/// Serves the post of the repair form on a stored query's page.
pub async fn post(
    Extension(state): Extension<ConsoleState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    extensions: http::Extensions,
    uri: Uri,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let outcome = repair(&state, &headers, &name, &fields).await;
    render(&state, (headers, extensions), uri, move || {
        provide_context(outcome.clone());
    })
    .await
}

/// Sends the version `fields` names of the stored query `name` again.
async fn repair(
    state: &ConsoleState,
    headers: &HeaderMap,
    name: &str,
    fields: &[(String, String)],
) -> RepairOutcome {
    let Ok(token) = crate::read::signed_in_with(state, headers) else {
        return RepairOutcome::Refused(Refusal::NotAuthenticated { code: None });
    };
    let locale = crate::locale::requested_from(state, headers);
    let invalid = |msg| {
        RepairOutcome::Refused(Refusal::Invalid {
            reason: tr_in(&locale, msg, &[]),
        })
    };
    let Some(replica) = state.settings().admin.first() else {
        return invalid(Msg::RepairNoAdmin);
    };
    if field(fields, crate::act::clear::CONFIRM).is_none() {
        return invalid(Msg::RepairConfirm);
    }
    let Some(version) = field(fields, "version") else {
        return invalid(Msg::RepairNameVersion);
    };
    let chosen: Vec<&str> = crate::act::fields_named(fields, "endpoint")
        .into_iter()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    let endpoints = if chosen.is_empty() || chosen.contains(&"*") {
        String::from("*")
    } else {
        chosen.join(", ")
    };
    if HeaderValue::from_str(&endpoints).is_err() {
        return invalid(Msg::RepairEndpointsIllegal);
    }
    let sent = Redistribution {
        name: name.to_owned(),
        version: version.to_owned(),
        endpoints,
    };
    let answered = state.gateway().repair_drift(replica, &token, &sent).await;
    RepairOutcome::Answered(acted(
        crate::read::ActionKind::Admin,
        (&replica.name, "drift repair"),
        answered,
        line,
    ))
}
