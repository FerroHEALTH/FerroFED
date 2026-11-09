// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members that serve the patient summary themselves, at their own
//! exchange-format endpoint (the `native` mode of the registry).
//!
//! The patient is resolved once for the whole summary, and each member the
//! section queries would reach that serves the patient summary itself is
//! left out of them and asked at its own endpoint instead, by its own
//! `ehr_id` alone ([`ferrofed_engine::single_node::native`]; §5.4.1, N33),
//! under the request's budget (§11.5). Its document comes back unchanged,
//! and one that does not answer is reported with its §11.1 status, which
//! makes the summary incomplete as a silent member of a query does (§11.3,
//! §11.4). No specification governs the `native` mode: our own design.

use std::sync::Arc;
use std::time::Instant;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{EhrAt, PatientLookup};
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_eehrxf::summary::document::{self, Unusable};
use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::fanout::{Completion, FanOutError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::forward::Forwarded;
use ferrofed_engine::single_node::native::{self as engine, SummaryAsked};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::native::NativeEndpoint;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use tokio::task::JoinSet;

use crate::access::Asked;
use crate::facade::answer::Failure;
use crate::facade::plan::Targets;
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// One member that serves the patient summary itself, as the summary
/// reaches it.
#[derive(Debug, Clone)]
pub(crate) struct Chosen {
    /// The member's Query API endpoint, whose client, credential and
    /// in-flight cap the request uses.
    pub(crate) via: EndpointId,
    /// The member.
    pub(crate) node: NodeId,
    /// Its native endpoint.
    pub(crate) native: EndpointId,
    /// Its own `ehr_id` for the patient.
    pub(crate) ehr_id: EhrId,
}

/// A member's own document: its bytes as it sent them, and the
/// `Bundle.identifier` they carry (FHIR R4 `bdl-9`).
///
/// `Debug` shows the answer's status and sizes and the identifier's system,
/// never the body or the identifier's value.
pub(crate) struct Document {
    forwarded: Forwarded,
    system: String,
    value: String,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("forwarded", &self.forwarded)
            .field("system", &self.system)
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Returns the member's answer, its bytes unchanged.
    pub(crate) fn forwarded(&self) -> &Forwarded {
        &self.forwarded
    }

    /// Returns the system and the value of the document's
    /// `Bundle.identifier`.
    pub(crate) fn identifier(&self) -> (&str, &str) {
        (&self.system, &self.value)
    }
}

/// What a member answered the request for its own patient summary, its
/// success read as a FHIR document `Bundle` with an identifier.
#[derive(Debug)]
pub(crate) enum Answer {
    /// A success that is a FHIR document `Bundle` with an identifier: the
    /// member's document, as it sent it.
    Document(Document),
    /// A success, of the status `status`, that is no FHIR document `Bundle`
    /// whose identifier has a system and a value (FHIR R4 `bdl-9`), so it
    /// says nothing about the patient.
    Unusable {
        /// The member's status.
        status: StatusCode,
        /// What the answer misses.
        reason: Unusable,
    },
    /// Every other answer, as the engine reports it: none, another status,
    /// or no answer at all.
    Unread(engine::Answer),
}

impl From<engine::Answer> for Answer {
    fn from(answer: engine::Answer) -> Self {
        let engine::Answer::Success(forwarded) = answer else {
            return Self::Unread(answer);
        };
        match document::identifier(forwarded.body()) {
            Ok((system, value)) => Self::Document(Document {
                forwarded,
                system,
                value,
            }),
            Err(reason) => Self::Unusable {
                status: forwarded.status(),
                reason,
            },
        }
    }
}

impl Answer {
    /// The §11.1 status of the member: `active` for its document or `404`,
    /// `node-error` for a success that is no document, and the engine's
    /// status otherwise.
    pub(crate) fn status(&self) -> EndpointStatus {
        match self {
            Self::Document(_) => EndpointStatus::Active,
            Self::Unusable { .. } => EndpointStatus::NodeError,
            Self::Unread(answer) => answer.status(),
        }
    }

    /// What the request showed of the member.
    pub(crate) fn contact(&self) -> Contact {
        match self {
            Self::Document(document) => Contact::Answered(document.forwarded().status()),
            Self::Unusable { status, .. } => Contact::Answered(*status),
            Self::Unread(answer) => answer.contact(),
        }
    }
}

/// What one such member answered.
#[derive(Debug)]
pub(crate) struct Native {
    /// The member, as the summary reached it.
    pub(crate) chosen: Chosen,
    /// What its own endpoint answered.
    pub(crate) answer: Answer,
}

/// Every member `targets` dispatches to that serves the patient summary
/// itself at an endpoint `snapshot` holds, with its own `ehr_id`, in
/// endpoint order.
pub(crate) fn chosen(snapshot: &RegistrySnapshot, targets: &Targets) -> Vec<Chosen> {
    chosen_for(snapshot, targets, Category::PatientSummary.code())
}

/// Every member `targets` dispatches to that serves the category `code`
/// itself at an endpoint `snapshot` holds, with its own `ehr_id`, in
/// endpoint order.
pub(crate) fn chosen_for(
    snapshot: &RegistrySnapshot,
    targets: &Targets,
    code: &str,
) -> Vec<Chosen> {
    targets
        .plan
        .dispatched()
        .filter_map(|via| {
            let node = snapshot.endpoint(via)?.node();
            let native: &NativeEndpoint = snapshot.native_for(node, code)?;
            // NOTE: §7.1; the plan dispatches only to a member the patient resolved at, so
            // a member with no ehr_id is legitimately not one the summary reaches.
            let (_, ehr_id) = targets.resolved.iter().find(|(held, _)| held == node)?;
            Some(Chosen {
                via: via.clone(),
                node: node.clone(),
                native: native.id().clone(),
                ehr_id: ehr_id.clone(),
            })
        })
        .collect()
}

/// The options a request to a member's own endpoint leaves under, for the
/// request `arrived`, refusing to send anything carrying one of `withheld`,
/// and the instant its overall budget runs out (§11.5).
///
/// # Errors
///
/// [`FanOutError::Clock`] when the clock cannot hold the budget.
pub(crate) fn options(
    federation: &Federation,
    arrived: &Arrived<'_>,
    withheld: Arc<Withheld>,
) -> Result<(DispatchOptions, tokio::time::Instant), FanOutError> {
    let budget = federation.budget();
    let overall = arrived
        .started
        .checked_add(budget.overall())
        .ok_or(FanOutError::Clock)?;
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    let options = DispatchOptions::new(per_node, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(withheld);
    Ok((options, tokio::time::Instant::from_std(overall)))
}

/// Asks every member `chosen` names at its own endpoint, at once, under
/// the budget of the request `arrived`, refusing to send anything carrying
/// one of `withheld`, and returns each answer in the order of `chosen`.
///
/// # Errors
///
/// [`Failure::FanOut`] when the clock cannot hold the budget, when a member
/// has no client or no native endpoint, or when a task failed.
pub(crate) async fn asked(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    chosen: Vec<Chosen>,
    withheld: Arc<Withheld>,
) -> Result<Vec<Native>, Failure> {
    let (options, until) = options(federation, arrived, withheld).map_err(Failure::FanOut)?;
    let mut tasks = JoinSet::new();
    for (position, member) in chosen.into_iter().enumerate() {
        let federation = Arc::clone(federation);
        let options = options.clone();
        tasks.spawn(async move {
            let snapshot = federation.snapshot();
            let (Some(client), Some(native)) = (
                federation.clients().get(&member.via),
                snapshot.native_endpoint(&member.native),
            ) else {
                return Err(FanOutError::UnknownEndpoint {
                    endpoint: member.via.clone(),
                });
            };
            let asked = SummaryAsked {
                native,
                ehr_id: &member.ehr_id,
                profile: EPS_COMPOSITION,
            };
            let answer = Answer::from(client.native_summary(asked, &options, until).await);
            if let Answer::Unusable { reason, .. } = &answer {
                tracing::warn!(
                    endpoint = member.native.as_str(),
                    %reason,
                    "a member's own summary is no document Bundle with an identifier (bdl-9)"
                );
            }
            Ok((
                position,
                Native {
                    chosen: member,
                    answer,
                },
            ))
        });
    }
    let mut answered = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let native = joined
            .map_err(|joined| Failure::FanOut(FanOutError::Task(joined)))?
            .map_err(Failure::FanOut)?;
        answered.push(native);
    }
    answered.sort_by_key(|(position, _)| *position);
    Ok(answered.into_iter().map(|(_, native)| native).collect())
}

/// The status a summary takes from what the members `natives` answered,
/// beside `status`, the one the section queries settled on, under
/// `completion` (§11.3, §11.4).
///
/// Under all-or-nothing, a member that did not answer fails the summary: a
/// `time-out` or `offline` member with `504`, which takes precedence, and a
/// `node-error` one with `424`. Under best-effort, it fails nothing and is
/// named in the document.
pub(crate) fn settled(
    natives: &[Native],
    status: StatusCode,
    completion: Completion,
) -> StatusCode {
    if completion == Completion::BestEffort {
        return status;
    }
    let silent = natives.iter().map(|native| native.answer.status());
    let mut settled = status;
    for member in silent {
        match member {
            EndpointStatus::Active => {}
            EndpointStatus::Offline | EndpointStatus::TimeOut => {
                return StatusCode::GATEWAY_TIMEOUT;
            }
            _ => {
                if settled == StatusCode::OK {
                    settled = StatusCode::FAILED_DEPENDENCY;
                }
            }
        }
    }
    settled
}

/// The access record's origin of every member of `natives` whose request
/// left the gateway (Annex II 3.2): with `delivered`, each whose own
/// document the answer carries contributed it; without, none did, as for
/// the gateway's own document, which leaves their data out.
pub(crate) fn origins<'a>(
    snapshot: &'a RegistrySnapshot,
    natives: &'a [Native],
    delivered: bool,
) -> impl Iterator<Item = Asked> + 'a {
    natives
        .iter()
        .filter(|native| native.answer.contact().left())
        .map(move |native| {
            let contributed = delivered && matches!(native.answer, Answer::Document(_));
            Asked {
                endpoint: native.chosen.native.as_str().to_owned(),
                node: Some(native.chosen.node.as_str().to_owned()),
                system_id: snapshot
                    .node(&native.chosen.node)
                    .map(|node| node.system_id().as_str().to_owned()),
                status: native.answer.status().as_str().to_owned(),
                rows: None,
                version: None,
                created_ehr: None,
                contributed,
                evidence: contributed.then(|| {
                    Evidence::reached(Basis::Returned, Vec::new())
                        .constructed(Category::PatientSummary)
                }),
            }
        })
}

/// The `ehr_id` each member of `natives` whose request left the gateway was
/// asked by, at its own endpoint, for the access record (Annex II 3.2).
pub(crate) fn ehrs(natives: &[Native]) -> impl Iterator<Item = EhrAt> + '_ {
    natives
        .iter()
        .filter(|native| native.answer.contact().left())
        .map(|native| EhrAt {
            endpoint: native.chosen.native.as_str().to_owned(),
            ehr_id: native.chosen.ehr_id.as_str().to_owned(),
            patient: PatientLookup::RequestNamed,
        })
}
