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
use ferrofed_registry::operator::ClearedEhrId;
use ferrofed_registry::operator::HeldBefore;

use crate::act::ReplicaAnswer;
use crate::act::fields::Fields;
use crate::read::{Fault, Refusal, Stop};

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
    /// The console could not run the clear, such as when its own server did
    /// not answer the page.
    Failed(Fault),
}

impl From<Stop> for ClearOutcome {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Refused(refusal) => Self::Refused(refusal),
            Stop::Fault(fault) => Self::Failed(fault),
        }
    }
}

/// The line of `cleared`.
fn line(cleared: ClearedEhrId) -> ClearLine {
    ClearLine {
        held: cleared.held,
        claimants: cleared.claimants,
    }
}

/// Clears the `ehr_id` the posted `fields` name.
///
/// The clear goes to the replica `replica` names, or to every replica for
/// [`EVERY_REPLICA`], once the dialog's confirmation step posted
/// `confirm`, as the operator of the request; a form that cannot be sent
/// and a replica's refusal are answers.
pub async fn clear_ehr_id(fields: &Fields) -> ClearOutcome {
    clear(
        fields.get("ehr_id").unwrap_or_default(),
        fields.get("replica").unwrap_or_default(),
        fields.get(CONFIRM).unwrap_or_default(),
    )
    .await
}

/// Clears the `ehr_id` the form names at the replicas it names.
async fn clear(ehr_id: &str, replica: &str, confirm: &str) -> ClearOutcome {
    use crate::locale::{Msg, tr};
    let Ok((state, token)) = crate::read::signed_in() else {
        return ClearOutcome::Refused(Refusal::NotAuthenticated { code: None });
    };
    let replicas = &state.settings().admin;
    let invalid = |msg| ClearOutcome::Refused(Refusal::Invalid { reason: tr(msg) });
    if replicas.is_empty() {
        return invalid(Msg::ClearNoAdmin);
    }
    if confirm.trim().is_empty() {
        return invalid(Msg::ClearConfirm);
    }
    let ehr_id = ehr_id.trim();
    if ehr_id.is_empty() {
        return invalid(Msg::ClearNameEhrId);
    }
    let chosen: Vec<_> = match replica.trim() {
        EVERY_REPLICA => replicas.iter().collect(),
        "" => Vec::new(),
        name => replicas
            .iter()
            .filter(|replica| replica.name == name)
            .collect(),
    };
    if chosen.is_empty() {
        return invalid(Msg::ClearChooseReplica);
    }
    let mut answers = Vec::new();
    for replica in chosen {
        let answered = state.gateway().clear_ehr_id(replica, &token, ehr_id).await;
        answers.push(crate::act::acted(
            crate::read::ActionKind::Admin,
            (&replica.name, "clear"),
            answered,
            line,
        ));
    }
    ClearOutcome::Answered(answers)
}
