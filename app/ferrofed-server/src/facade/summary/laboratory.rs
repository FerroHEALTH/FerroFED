// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory reports a document list gathers beside the patient
//! summary (Regulation (EU) 2025/327 Art 14(1)(e)), and how their answers
//! settle the list's completeness (§11.3, §11.4).
//!
//! A member declared `native` for `Laboratory-Reports` is asked at its own
//! endpoint ([`super::own`]) and left out of the laboratory report query,
//! which every other member is asked when the gateway maps the category
//! ([`Laboratory::Mapped`]).

use std::collections::BTreeSet;
use std::sync::Arc;

use ehds_logging::category::Category;
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

/// The laboratory reports a list gathers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Laboratory {
    /// None.
    #[default]
    Unlisted,
    /// Each member's own, from the members that serve them themselves.
    Own,
    /// Those, and the gateway's, from every other member's compositions.
    Mapped,
}

/// The members `located` dispatches to that serve laboratory reports at
/// their own endpoint, in endpoint order, and, when `laboratory` is
/// [`Laboratory::Mapped`], what every other member answered the laboratory
/// report query for the patient `system`|`value`.
///
/// # Errors
///
/// The [`Unsummarised`] of a query that could not be run.
pub(super) async fn queried(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, laboratory): (&Located, Laboratory),
    patient: (&str, &str),
) -> Result<(Vec<Chosen>, Vec<(EndpointId, Answered)>), Unsummarised> {
    if laboratory == Laboratory::Unlisted {
        return Ok((Vec::new(), Vec::new()));
    }
    let own = native::chosen_for(
        federation.snapshot(),
        &located.targets,
        Category::MedicalTestResult.code(),
    );
    let answered = if laboratory == Laboratory::Mapped {
        laboratory_reports(federation, arrived, (located, &own), patient).await?
    } else {
        Vec::new()
    };
    Ok((own, answered))
}

/// Keeps in `gathered` what the members answered the laboratory report
/// query, `laboratory`, and what those that serve documents themselves
/// answered, `own`, with every endpoint a request left for.
pub(super) fn kept(
    gathered: &mut super::Gathered,
    laboratory: Vec<(EndpointId, Answered)>,
    own: Vec<Own>,
) {
    for (endpoint, answered) in &laboratory {
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
    gathered.laboratory = laboratory;
    gathered.own = own;
}

/// What every member the plan of `located` dispatches to, but those of
/// `own`, answered the laboratory report query for the patient
/// `system`|`value` ([`ferrofed_eehrxf::laboratory`]), run as the
/// medication queries are.
async fn laboratory_reports(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, own): (&Located, &[Chosen]),
    (system, value): (&str, &str),
) -> Result<Vec<(EndpointId, Answered)>, Unsummarised> {
    let name = ferrofed_eehrxf::laboratory::name()
        .map_err(|_unnamed| Unsummarised::Internal("the laboratory query has no qualified name"))?;
    // NOTE: Federation Tier § The four identifiers, N33: a member asked at its own endpoint for
    // its laboratory reports is asked no laboratory query, so no report is listed twice.
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
        (
            (system, value),
            (name.as_str(), ferrofed_eehrxf::laboratory::aql()),
        ),
        &members,
    )
    .await
    .map_err(|unprescribed| match unprescribed {
        Unprescribed::Failure(failure) => Unsummarised::Failure(failure),
        _ => Unsummarised::Internal("the laboratory query could not be run"),
    })
}

/// The status `status` becomes once the laboratory report query's answers
/// `laboratory` are settled under `completion`: a silent member fails it
/// under all-or-nothing, `504` for one in `time-out` or `offline` and `424`
/// otherwise (§11.3, §11.4).
pub(super) fn laboratory_settled(
    laboratory: &[(EndpointId, Answered)],
    status: StatusCode,
    completion: Completion,
) -> StatusCode {
    let queried = laboratory.iter().map(|(_, answered)| match answered {
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
