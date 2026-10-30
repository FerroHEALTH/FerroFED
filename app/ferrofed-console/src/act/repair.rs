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


use crate::act::ReplicaAnswer;
use crate::act::fields::Fields;
use crate::act::query::EndpointLine;
use crate::gateway::admin::Distribution;
use crate::read::{Fault, Refusal, Stop};

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
    /// The console could not run the repair, such as when its own server
    /// did not answer the page.
    Failed(Fault),
}

impl From<Stop> for RepairOutcome {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Refused(refusal) => Self::Refused(refusal),
            Stop::Fault(fault) => Self::Failed(fault),
        }
    }
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

/// Sends the version the posted `fields` name of the stored query `name`
/// again.
///
/// It goes to the members `endpoint` names, `*` for every active member,
/// once the dialog's confirmation step posted `confirm`, through the first
/// replica, as the operator of the request; a form that cannot be sent and
/// a replica's refusal are answers.
pub async fn repair_drift(name: &str, fields: &Fields) -> RepairOutcome {
    repair(
        name,
        fields.get("version").unwrap_or_default(),
        fields.get("endpoint").unwrap_or_default(),
        fields.get("confirm").unwrap_or_default(),
    )
    .await
}

/// Sends the version the form names of the stored query `name` again.
async fn repair(name: &str, version: &str, endpoint: &str, confirm: &str) -> RepairOutcome {
    use crate::gateway::admin::Redistribution;
    use crate::locale::{Msg, tr};
    let Ok((state, token)) = crate::read::signed_in() else {
        return RepairOutcome::Refused(Refusal::NotAuthenticated { code: None });
    };
    let invalid = |msg| RepairOutcome::Refused(Refusal::Invalid { reason: tr(msg) });
    let Some(replica) = state.settings().admin.first() else {
        return invalid(Msg::RepairNoAdmin);
    };
    if confirm.trim().is_empty() {
        return invalid(Msg::RepairConfirm);
    }
    let version = version.trim();
    if version.is_empty() {
        return invalid(Msg::RepairNameVersion);
    }
    let chosen: Vec<&str> = endpoint
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect();
    let endpoints = if chosen.is_empty() || chosen.contains(&"*") {
        String::from("*")
    } else {
        chosen.join(", ")
    };
    if http::HeaderValue::from_str(&endpoints).is_err() {
        return invalid(Msg::RepairEndpointsIllegal);
    }
    let sent = Redistribution {
        name: name.to_owned(),
        version: version.to_owned(),
        endpoints,
    };
    let answered = state.gateway().repair_drift(replica, &token, &sent).await;
    RepairOutcome::Answered(crate::act::acted(
        crate::read::ActionKind::Admin,
        (&replica.name, "drift repair"),
        answered,
        line,
    ))
}
