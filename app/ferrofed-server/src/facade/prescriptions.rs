// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search of a patient's electronic prescriptions over the members that
//! serve them (Regulation (EU) 2025/327 Art 14(1)(b)).
//!
//! The patient is located once ([`super::located`]), and each member the
//! deployment declares for prescriptions that the patient resolved at is
//! asked at its own endpoint, by its own `ehr_id` alone
//! ([`ferrofed_engine::single_node::native_search`]; §5.4.1, N33), under the
//! request's budget (§11.5), its `next` links followed under its own base
//! until its last page. A member that does not answer a page, answers one
//! the face cannot read, or names a page outside its base is reported with
//! its §11.1 status and never as a member with no prescription (§11.2): under
//! all-or-nothing it fails the search, `504` for a member in `time-out` or
//! `offline` and `424` otherwise, and under `partial` it is named in the
//! answer (§11.3, §11.4). The page limit is our own design: no specification
//! governs it.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup};
use ferrofed_eehrxf::prescriptions::RESOURCE;
use ferrofed_eehrxf::prescriptions::page::{self, Page};
use ferrofed_eehrxf::prescriptions::searchset::Assembled;
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::fanout::{Completion, FanOutError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_search::{Elsewhere, Next, SearchAsked, Searched};
use ferrofed_registry::id::{EndpointId, NodeId};
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use secrecy::SecretString;
use tokio::task::JoinSet;

use crate::access::{Accessed, Asked};
use crate::facade::answer::{self, Failure};
use crate::facade::located::{Located, located};
use crate::facade::native::{self, Chosen};
use crate::facade::request::Arrived;
use crate::facade::summary::Unsummarised;
use crate::federation::Federation;

/// The operation an access record names for a prescription search.
pub(crate) const OPERATION: &str = RESOURCE;

/// The most pages one member's search is followed for.
const PAGES: usize = 64;

/// Why a member's success is no page the search can pass through.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Unread {
    /// The page is no `searchset` of prescriptions.
    #[error(transparent)]
    Page(page::Unusable),
    /// The page's `next` link leaves the member's own endpoint.
    #[error(transparent)]
    Next(Elsewhere),
    /// The member answered more pages than the search follows.
    #[error("the member answered more than {PAGES} pages")]
    Pages,
    /// Asked for one prescription by its id, the member answered another.
    #[error("the member answered a prescription other than the one asked for")]
    Other,
}

/// What one member answered the search with.
#[derive(Debug)]
pub(crate) enum Held {
    /// Every page of the member's search.
    Found(Vec<Page>),
    /// A success, of the status `status`, that the search cannot pass
    /// through, which says nothing about the patient.
    Unusable {
        /// The member's status.
        status: StatusCode,
        /// What the answer misses.
        reason: Unread,
    },
    /// No page: the member's answer to the page it did not give.
    Silent(Searched),
}

impl Held {
    /// The §11.1 status of the member: `active` when it gave every page, and
    /// otherwise the status of the page it did not give.
    pub(crate) fn status(&self) -> EndpointStatus {
        match self {
            Self::Found(_) => EndpointStatus::Active,
            Self::Unusable { .. } => EndpointStatus::NodeError,
            Self::Silent(searched) => searched.status(),
        }
    }

    /// Whether a request left the gateway for the member.
    pub(crate) fn left(&self) -> bool {
        match self {
            Self::Found(_) | Self::Unusable { .. } => true,
            Self::Silent(searched) => searched.contact().left(),
        }
    }

    /// The number of prescriptions the member answered with.
    pub(crate) fn prescriptions(&self) -> usize {
        match self {
            Self::Found(pages) => pages.iter().map(|page| page.prescriptions.len()).sum(),
            Self::Unusable { .. } | Self::Silent(_) => 0,
        }
    }
}

/// One member asked, and what it answered.
#[derive(Debug)]
pub(crate) struct Member {
    /// The member, as the search reached it.
    pub(crate) chosen: Chosen,
    /// What it answered.
    pub(crate) held: Held,
}

/// What the search gathered.
#[derive(Debug)]
pub(crate) struct Prescribed {
    /// Each member asked, in endpoint order.
    pub(crate) members: Vec<Member>,
    /// The members whose consent pre-filter denial was set aside for an
    /// emergency purpose.
    pub(crate) set_aside: BTreeSet<NodeId>,
    /// The Query API endpoint of each member declared for prescriptions that
    /// the resolver could not answer for, in endpoint order.
    pub(crate) unresolved: Vec<EndpointId>,
}

/// Why the search gives no prescriptions.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Unprescribed {
    /// The request was refused before anything was sent, or the fan-out
    /// failed on the gateway's side.
    #[error(transparent)]
    Failure(Failure),
    /// The gateway could not run the search, for a reason of its own that
    /// names no value.
    #[error("the search could not be run: {0}")]
    Internal(&'static str),
    /// No member that serves prescriptions holds the patient, or none may
    /// say so (§11.3), with what was gathered when a consent denial was set
    /// aside, which its access record names.
    #[error("no member holds prescriptions for this identifier")]
    NotFound(Option<Box<Prescribed>>),
    /// Under all-or-nothing, a member did not answer (§11.3, §11.4); the
    /// answer names each such member.
    #[error("the search is incomplete")]
    Incomplete {
        /// The status the answer takes.
        status: StatusCode,
        /// Each member's own endpoint that did not answer, with its §11.1
        /// status.
        silent: Vec<(String, String)>,
        /// What was gathered, for the access record.
        prescribed: Box<Prescribed>,
    },
}

impl From<Unsummarised> for Unprescribed {
    fn from(unlocated: Unsummarised) -> Self {
        match unlocated {
            Unsummarised::Failure(Failure::NoDestination) => Self::NotFound(None),
            Unsummarised::Failure(failure) => Self::Failure(failure),
            Unsummarised::Internal(what) => Self::Internal(what),
            // NOTE: no specification governs this: our own design; locating a patient asks no
            // section query, so it never ends in a summary's own outcomes.
            Unsummarised::NotFound(_) | Unsummarised::Incomplete { .. } => {
                Self::Internal("the patient could not be located")
            }
        }
    }
}

/// Searches the prescriptions of the patient `system`|`value`, as `arrived`
/// asks, at every member of `declared` the patient resolved at.
///
/// # Errors
///
/// The [`Unprescribed`] that says why no prescription can be answered.
pub(crate) async fn search(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    patient: (&str, &str),
    declared: &BTreeSet<String>,
) -> Result<Prescribed, Unprescribed> {
    let Located {
        completion,
        targets,
        ..
    } = located(federation, arrived, patient).await?;
    let chosen: Vec<Chosen> = native::chosen_for(
        federation.snapshot(),
        &targets,
        Category::ElectronicPrescription.code(),
    )
    .into_iter()
    .filter(|member| declared.contains(member.node.as_str()))
    .collect();
    // NOTE: no specification governs this (§11.3 covers only an answered lookup): our own design;
    // only a member serving prescriptions whose resolution failed could hide one.
    let snapshot = federation.snapshot();
    let unresolved: Vec<EndpointId> = targets
        .unresolved
        .iter()
        .filter(|endpoint| {
            snapshot
                .endpoint(endpoint)
                .is_some_and(|held| declared.contains(held.node().as_str()))
        })
        .cloned()
        .collect();
    let mut prescribed = Prescribed {
        members: Vec::new(),
        set_aside: targets.set_aside.clone(),
        unresolved,
    };
    // NOTE: Federation Tier §11.3, Regulation (EU) 2025/327 Art 8: a patient no member serving
    // prescriptions holds answers as one none may disclose does, so neither is told apart.
    if chosen.is_empty() && prescribed.unresolved.is_empty() {
        let set_aside = (!prescribed.set_aside.is_empty()).then(|| Box::new(prescribed));
        return Err(Unprescribed::NotFound(set_aside));
    }
    prescribed.members = asked(federation, arrived, chosen, targets.plan.withheld()).await?;
    for member in &prescribed.members {
        if let Held::Unusable { status, reason } = &member.held {
            tracing::warn!(
                endpoint = member.chosen.native.as_str(),
                status = status.as_u16(),
                reason = %reason,
                request_id = %arrived.outbound,
                "a member answered a prescription search the face cannot pass through"
            );
        }
    }
    let mut status = answer::fan_out::settled(
        StatusCode::OK,
        !prescribed.unresolved.is_empty(),
        completion,
    );
    if completion == Completion::AllOrNothing {
        for member in &prescribed.members {
            match member.held.status() {
                EndpointStatus::Active => {}
                EndpointStatus::Offline | EndpointStatus::TimeOut => {
                    status = StatusCode::GATEWAY_TIMEOUT;
                }
                _ if status == StatusCode::OK => status = StatusCode::FAILED_DEPENDENCY,
                _ => {}
            }
        }
    }
    if status != StatusCode::OK {
        let unresolved = prescribed.unresolved.iter().map(|endpoint| {
            (
                endpoint.as_str().to_owned(),
                EndpointStatus::NotResolved.as_str().to_owned(),
            )
        });
        let silent = prescribed
            .members
            .iter()
            .filter(|member| member.held.status() != EndpointStatus::Active)
            .map(|member| {
                (
                    member.chosen.native.as_str().to_owned(),
                    member.held.status().as_str().to_owned(),
                )
            })
            .chain(unresolved)
            .collect();
        return Err(Unprescribed::Incomplete {
            status,
            silent,
            prescribed: Box::new(prescribed),
        });
    }
    Ok(prescribed)
}

/// Asks every member `chosen` names at its own endpoint, at once, under the
/// budget of the request `arrived`, refusing to send anything carrying one
/// of `withheld`, and returns each answer in the order of `chosen`.
pub(crate) async fn asked(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    chosen: Vec<Chosen>,
    withheld: Arc<Withheld>,
) -> Result<Vec<Member>, Unprescribed> {
    let budget = federation.budget();
    let overall = arrived
        .started
        .checked_add(budget.overall())
        .ok_or(Unprescribed::Failure(Failure::FanOut(FanOutError::Clock)))?;
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    let options = DispatchOptions::new(per_node, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(withheld);
    let until = tokio::time::Instant::from_std(overall);
    let mut tasks = JoinSet::new();
    for (position, member) in chosen.into_iter().enumerate() {
        let federation = Arc::clone(federation);
        let options = options.clone();
        tasks.spawn(async move {
            let held = paged(&federation, (&member, None), &options, until).await;
            held.map(|held| {
                (
                    position,
                    Member {
                        chosen: member,
                        held,
                    },
                )
            })
        });
    }
    let mut answered = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let member = joined
            .map_err(|joined| Unprescribed::Failure(Failure::FanOut(FanOutError::Task(joined))))?
            .map_err(|failure| Unprescribed::Failure(Failure::FanOut(failure)))?;
        answered.push(member);
    }
    answered.sort_by_key(|(position, _)| *position);
    Ok(answered.into_iter().map(|(_, member)| member).collect())
}

/// Every page of `member`'s search, for the one prescription of the logical
/// id `id` when it is given, under `options`, abandoned when the overall
/// budget `until` runs out.
///
/// # Errors
///
/// [`FanOutError::UnknownEndpoint`] when the member has no client or no
/// native endpoint.
pub(crate) async fn paged(
    federation: &Federation,
    (member, id): (&Chosen, Option<&str>),
    options: &DispatchOptions,
    until: tokio::time::Instant,
) -> Result<Held, FanOutError> {
    let snapshot = federation.snapshot();
    let (Some(client), Some(endpoint)) = (
        federation.clients().get(&member.via),
        snapshot.native_endpoint(&member.native),
    ) else {
        return Err(FanOutError::UnknownEndpoint {
            endpoint: member.via.clone(),
        });
    };
    let mut pages = Vec::new();
    let mut next: Option<Next> = None;
    loop {
        if pages.len() >= PAGES {
            return Ok(Held::Unusable {
                status: StatusCode::OK,
                reason: Unread::Pages,
            });
        }
        let asked = SearchAsked {
            native: endpoint,
            ehr_id: &member.ehr_id,
            resource: RESOURCE,
            id,
            next: next.as_ref(),
        };
        let page = match client.native_search(asked, options, until).await {
            Searched::Page(page) => page,
            silent => return Ok(Held::Silent(silent)),
        };
        let status = page.status();
        let read = match Page::read(page.body(), endpoint.url()) {
            Ok(read) => read,
            Err(reason) => {
                return Ok(Held::Unusable {
                    status,
                    reason: Unread::Page(reason),
                });
            }
        };
        let following = read.next.clone();
        pages.push(read);
        let Some(following) = following else {
            return Ok(Held::Found(pages));
        };
        match Next::under(endpoint.url(), &following) {
            Ok(following) => next = Some(following),
            Err(elsewhere) => {
                return Ok(Held::Unusable {
                    status,
                    reason: Unread::Next(elsewhere),
                });
            }
        }
    }
}

/// The access record's facts of the search for the patient
/// `system`|`value` that `prescribed` shows, the page `assembled` answered,
/// or `None` when no request left the gateway or the federation keeps no
/// access log (Annex II 3.2).
///
/// The request serves the electronic prescription category by construction
/// (Art 14(1)(b)), and each member whose request left the gateway is an
/// origin, contributing when the page carries one of its prescriptions.
pub(crate) fn accessed(
    federation: &Federation,
    prescribed: &Prescribed,
    (system, value): (&str, &str),
    assembled: Option<&Assembled>,
) -> Option<Accessed> {
    let log = federation.access_log()?;
    let left: Vec<(usize, &Member)> = prescribed
        .members
        .iter()
        .enumerate()
        .filter(|(_, member)| member.held.left())
        .collect();
    if left.is_empty() && prescribed.set_aside.is_empty() {
        return None;
    }
    let evidence = || {
        Evidence::reached(Basis::Returned, Vec::new()).constructed(Category::ElectronicPrescription)
    };
    let snapshot = federation.snapshot();
    let answering: Vec<usize> = prescribed
        .members
        .iter()
        .enumerate()
        .filter(|(_, member)| matches!(member.held, Held::Found(_)))
        .map(|(index, _)| index)
        .collect();
    let origins = left
        .iter()
        .map(|(index, member)| {
            let position = answering.iter().position(|answered| answered == index);
            let contributed = position
                .and_then(|position| assembled?.delivered.get(&position))
                .is_some_and(|carried| *carried > 0);
            Asked {
                endpoint: member.chosen.native.as_str().to_owned(),
                node: Some(member.chosen.node.as_str().to_owned()),
                system_id: snapshot
                    .node(&member.chosen.node)
                    .map(|node| node.system_id().as_str().to_owned()),
                status: member.held.status().as_str().to_owned(),
                rows: Some(member.held.prescriptions()),
                version: None,
                created_ehr: None,
                contributed,
                evidence: contributed.then(evidence),
            }
        })
        .collect();
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Query,
        operation: OPERATION,
        resource: None,
        // NOTE: BALP 1.1.4 entity:query holds the request, which a FHIR search names in its
        // parameters.
        query: Some(SecretString::from(format!(
            "patient.identifier={system}|{value}"
        ))),
        stored_query: None,
        subject: DataSubject {
            patient: Some(PatientIdentifier {
                namespace: system.to_owned(),
                value: SecretString::from(value.to_owned()),
            }),
            ehrs: left
                .iter()
                .map(|(_, member)| EhrAt {
                    endpoint: member.chosen.native.as_str().to_owned(),
                    ehr_id: member.chosen.ehr_id.as_str().to_owned(),
                    patient: PatientLookup::RequestNamed,
                })
                .collect(),
            population: None,
        },
        evidence: evidence(),
        delivered: Some(assembled.map_or(0, Assembled::carried)),
        origins,
        consent_set_aside: prescribed
            .set_aside
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect(),
        inserted: None,
        transformations: Vec::new(),
    })
}
