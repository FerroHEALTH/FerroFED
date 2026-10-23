// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The clear of a held `ehr_id`, posted from Integrity.
//!
//! The form posts the `ehr_id` and the replica, or every replica, and each
//! replica named clears it from its `ehr_id` index and its resolution
//! bindings, one after another, as the operator.
//!
//! The page answers with one line per replica. An `ehr_id` is a routing id,
//! never a patient identifier (§5.4.1, N33), and the clear is recorded with
//! the operator by the gateway.

use axum::Extension;
use axum::extract::Form;
use axum::response::Response;
use ferrofed_registry::operator::{ClearedEhrId, HeldBefore};
use http::{HeaderMap, Uri};
use leptos::context::provide_context;

use crate::act::{ReplicaAnswer, acted, field, render};
use crate::read::Refusal;
use crate::server::ConsoleState;

/// The field the dialog's confirmation step posts, which an action needs
/// before it asks any replica.
pub const CONFIRM: &str = "confirm";

/// The value of the replica field that names every replica.
pub const EVERY_REPLICA: &str = "*";

/// What a replica reports of a clear.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearLine {
    /// What the replica held before.
    pub held: HeldBefore,
    /// Every member the index or a binding named, by `node_id`.
    pub claimants: Vec<String>,
}

/// The outcome of a clear the page draws: the answer of each replica, or
/// why no replica was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearOutcome {
    /// The replicas asked, each with its answer.
    Answered(Vec<ReplicaAnswer<ClearLine>>),
    /// The form cannot be sent as it stands.
    Refused(Refusal),
}

/// The line of `cleared`.
fn line(cleared: ClearedEhrId) -> ClearLine {
    ClearLine {
        held: cleared.held,
        claimants: cleared.claimants,
    }
}

/// Serves the post of the clear form on Integrity.
pub async fn post(
    Extension(state): Extension<ConsoleState>,
    headers: HeaderMap,
    extensions: http::Extensions,
    uri: Uri,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let outcome = clear(&state, &headers, &fields).await;
    render(&state, (headers, extensions), uri, move || {
        provide_context(outcome.clone());
    })
    .await
}

/// Clears the `ehr_id` the form `fields` names at the replicas it names.
async fn clear(
    state: &ConsoleState,
    headers: &HeaderMap,
    fields: &[(String, String)],
) -> ClearOutcome {
    let token = match crate::read::signed_in_with(state, headers) {
        Ok(token) => token,
        Err(_signed_out) => {
            return ClearOutcome::Refused(Refusal::NotAuthenticated { code: None });
        }
    };
    let replicas = &state.settings().admin;
    let locale = crate::locale::requested_from(state, headers);
    let invalid = |msg| {
        ClearOutcome::Refused(Refusal::Invalid {
            reason: crate::locale::tr_in(&locale, msg, &[]),
        })
    };
    if replicas.is_empty() {
        return invalid(crate::locale::Msg::ClearNoAdmin);
    }
    if field(fields, CONFIRM).is_none() {
        return invalid(crate::locale::Msg::ClearConfirm);
    }
    let Some(ehr_id) = field(fields, "ehr_id") else {
        return invalid(crate::locale::Msg::ClearNameEhrId);
    };
    let chosen: Vec<_> = match field(fields, "replica") {
        Some(EVERY_REPLICA) => replicas.iter().collect(),
        Some(name) => replicas
            .iter()
            .filter(|replica| replica.name == name)
            .collect(),
        None => Vec::new(),
    };
    if chosen.is_empty() {
        return invalid(crate::locale::Msg::ClearChooseReplica);
    }
    let mut answers = Vec::new();
    for replica in chosen {
        let answered = state.gateway().clear_ehr_id(replica, &token, ehr_id).await;
        answers.push(acted(
            crate::read::ActionKind::Admin,
            (&replica.name, "clear"),
            answered,
            line,
        ));
    }
    ClearOutcome::Answered(answers)
}
