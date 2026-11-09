// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reports a document list gathers beside the patient summary
//! (Regulation (EU) 2025/327 Art 14(1)(e) and (f)), and how their answers
//! settle the list's completeness (§11.3, §11.4).
//!
//! A member declared `native` for `Laboratory-Reports` is asked at its own
//! endpoint ([`super::own`]). Every category the gateway writes from a
//! member's composition ([`Written`]) has its stored query asked of every
//! member that does not serve the category itself.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ehds_logging::category::Category;
use ferrofed_eehrxf::report::Written;
use ferrofed_engine::fanout::Completion;
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use super::Unsummarised;
use super::own::Own;
use crate::facade::located::Located;
use crate::facade::mapped::{self, Answered};
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions::Unprescribed;
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// What each member answered the stored query of each written category, by
/// category and then in endpoint order.
pub(crate) type Answers = BTreeMap<Written, Vec<(EndpointId, Answered)>>;

/// The members `located` dispatches to that serve laboratory reports at
/// their own endpoint, in endpoint order, when `laboratory` asks for them,
/// and what every member that does not serve a category of `written`
/// itself answered that category's stored query for the patient
/// `system`|`value`.
///
/// # Errors
///
/// The [`Unsummarised`] of a query that could not be run.
pub(super) async fn queried(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, laboratory, written): (&Located, bool, &BTreeSet<Written>),
    patient: (&str, &str),
) -> Result<(Vec<Chosen>, Answers), Unsummarised> {
    let own = if laboratory {
        native::chosen_for(
            federation.snapshot(),
            &located.targets,
            Category::MedicalTestResult.code(),
        )
    } else {
        Vec::new()
    };
    let mut answers = Answers::new();
    for category in written {
        let serving =
            native::chosen_for(federation.snapshot(), &located.targets, category.category());
        let answered = reports(
            federation,
            arrived,
            (located, &serving),
            (*category, patient),
        )
        .await?;
        answers.insert(*category, answered);
    }
    Ok((own, answers))
}

/// Keeps in `gathered` what the members answered the written categories'
/// queries, `written`, and what those that serve documents themselves
/// answered, `own`, with every endpoint a request left for.
pub(super) fn kept(gathered: &mut super::Gathered, written: Answers, own: Vec<Own>) {
    for (endpoint, answered) in written.values().flatten() {
        if matches!(
            answered,
            Answered::Rows(_) | Answered::Silent { left: true, .. }
        ) {
            gathered.sent.insert(endpoint.clone());
        }
    }
    for member in &own {
        if member.held.left() {
            gathered.sent.insert(member.chosen.native.clone());
        }
    }
    gathered.written = written;
    gathered.own = own;
}

/// What every member the plan of `located` dispatches to, but those of
/// `own`, answered the stored query of `category` for the patient
/// `system`|`value`, run as the medication queries are.
async fn reports(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, own): (&Located, &[Chosen]),
    (category, (system, value)): (Written, (&str, &str)),
) -> Result<Vec<(EndpointId, Answered)>, Unsummarised> {
    let name = category
        .name()
        .map_err(|_unnamed| Unsummarised::Internal("a report query has no qualified name"))?;
    // NOTE: Federation Tier § The four identifiers, N33: a member asked at its own endpoint for
    // its reports of a category is asked no query of it, so no report is listed twice.
    let members: BTreeSet<EndpointId> = located
        .targets
        .plan
        .dispatched()
        .filter(|endpoint| !own.iter().any(|member| &member.via == *endpoint))
        .cloned()
        .collect();
    if members.is_empty() {
        return Ok(Vec::new());
    }
    mapped::queried(
        federation,
        arrived,
        located,
        ((system, value), (&name, category.aql())),
        &members,
    )
    .await
    .map_err(|unprescribed| match unprescribed {
        Unprescribed::Failure(failure) => Unsummarised::Failure(failure),
        _ => Unsummarised::Internal("a report query could not be run"),
    })
}

/// The status `status` becomes once the written categories' answers
/// `written` are settled under `completion`: a silent member fails it under
/// all-or-nothing, `504` for one in `time-out` or `offline` and `424`
/// otherwise (§11.3, §11.4).
pub(super) fn written_settled(
    written: &Answers,
    status: StatusCode,
    completion: Completion,
) -> StatusCode {
    let queried = written
        .values()
        .flatten()
        .map(|(_, answered)| match answered {
            Answered::Rows(_) => EndpointStatus::Active,
            Answered::Silent { status, .. } => *status,
        });
    settled_by(queried, status, completion)
}

/// The status `status` becomes once the members of the §11.1 statuses
/// `members` are settled under `completion`.
pub(super) fn settled_by(
    members: impl Iterator<Item = EndpointStatus>,
    mut status: StatusCode,
    completion: Completion,
) -> StatusCode {
    if completion != Completion::AllOrNothing {
        return status;
    }
    for member in members {
        match member {
            EndpointStatus::TimeOut | EndpointStatus::Offline => {
                status = StatusCode::GATEWAY_TIMEOUT;
            }
            EndpointStatus::Active => {}
            _ if status == StatusCode::OK => status = StatusCode::FAILED_DEPENDENCY,
            _ => {}
        }
    }
    status
}
