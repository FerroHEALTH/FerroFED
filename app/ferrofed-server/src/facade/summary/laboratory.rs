// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report query a document list runs beside the patient
//! summary's (Regulation (EU) 2025/327 Art 14(1)(e)), and how its answers
//! settle the list's completeness (§11.3, §11.4).

use std::collections::BTreeSet;
use std::sync::Arc;

use ferrofed_engine::fanout::Completion;
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use super::Unsummarised;
use crate::facade::located::Located;
use crate::facade::mapped::{self, Answered};
use crate::facade::prescriptions::Unprescribed;
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// What every member the plan of `located` dispatches to answered the
/// laboratory report query for the patient `system`|`value`
/// ([`ferrofed_eehrxf::laboratory`]), run as the medication queries are.
pub(super) async fn laboratory_reports(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    located: &Located,
    (system, value): (&str, &str),
) -> Result<Vec<(EndpointId, Answered)>, Unsummarised> {
    let name = ferrofed_eehrxf::laboratory::name()
        .map_err(|_unnamed| Unsummarised::Internal("the laboratory query has no qualified name"))?;
    let members: BTreeSet<EndpointId> = located.targets.plan.dispatched().cloned().collect();
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
    mut status: StatusCode,
    completion: Completion,
) -> StatusCode {
    if completion != Completion::AllOrNothing {
        return status;
    }
    for (_, answered) in laboratory {
        match answered {
            Answered::Silent {
                status: EndpointStatus::TimeOut | EndpointStatus::Offline,
                ..
            } => status = StatusCode::GATEWAY_TIMEOUT,
            Answered::Silent { .. } if status == StatusCode::OK => {
                status = StatusCode::FAILED_DEPENDENCY;
            }
            Answered::Rows(_) | Answered::Silent { .. } => {}
        }
    }
    status
}
