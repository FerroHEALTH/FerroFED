// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The prescriptions of the members `[[fhir.serve]]` declares in the map
//! mode, read from their openEHR medication orders over ITS-REST
//! (Regulation (EU) 2025/327 Art 14(1)(b); A102 point 1).
//!
//! The search runs the reserved query `eu.ferrofed.eehrxf::medication-orders`
//! ([`ferrofed_eehrxf::medication`]) over the members the patient resolved
//! at, as the patient summary runs a section query: read and analysed as a
//! stored query bound to the patient through `$patient` and `$namespace`
//! alone, planned over the resolution the search made once
//! ([`plan::retarget`]), sent to each map-mode member by its own `ehr_id`
//! through the rewrite and the outbound identifier-hygiene gate (§5.4.1,
//! N33), under the request's budget (§11.5). Of what a member answers, only a
//! composition whose uid's `creating_system_id` is the member's own
//! `system_id` is its own prescription (A102 point 1; BASE
//! `OBJECT_VERSION_ID.creating_system_id`); each is mapped by the member's
//! mapping into one page ([`ferrofed_eehrxf::prescriptions::mapped`]). A
//! member that does not answer is reported with its §11.1 status, and one
//! whose composition does not map as a member whose answer the face cannot
//! pass through, never as a member with no prescription (§11.2).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use ferrofed_eehrxf::medication::Medication;
use ferrofed_eehrxf::prescriptions::mapped::MapMode;
use ferrofed_eehrxf::prescriptions::page::Page;
use ferrofed_engine::dispatch::{Contact, DispatchOptions};
use ferrofed_engine::fanout::FanOutError;
use ferrofed_engine::fanout::reader::RowReader;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError};
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_identity::role::resolver::Identification;
use ferrofed_registry::id::{EndpointId, SystemId};
use ferrofed_registry::snapshot::{Node, RegistrySnapshot};
use http::header::ACCEPT;
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use openehr_base::prelude::ObjectVersionId;
use openehr_federation::dedup::DedupMode;
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::path_segment;
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_its::rest::stored_query::QualifiedQueryName;
use secrecy::ExposeSecret;
use uuid::Uuid;

use crate::facade::answer::{self, Failure};
use crate::facade::located::{self, Located};
use crate::facade::native::Chosen;
use crate::facade::prescriptions::{Composed, Held, Member, Unprescribed, Unread};
use crate::facade::request::Arrived;
use crate::facade::summary::{compositions, patient_query};
use crate::facade::{follow_up, plan};
use crate::federation::Federation;

/// The prescriptions of the patient `system`|`value`, as `arrived` asks, at
/// every member of `mapped` the plan of `located` dispatches to, each mapped
/// by its own mapping at `now`, a FHIR `instant`.
///
/// # Errors
///
/// The [`Unprescribed`] of a query the gateway could not run.
pub(crate) async fn asked(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    located: &Located,
    (patient, now): ((&str, &str), &str),
    mapped: &BTreeMap<String, MapMode>,
) -> Result<Vec<Member>, Unprescribed> {
    let snapshot = federation.snapshot();
    let targets = &located.targets;
    let members: BTreeSet<EndpointId> = targets
        .plan
        .dispatched()
        .filter(|endpoint| {
            snapshot
                .endpoint(endpoint)
                .is_some_and(|held| mapped.contains_key(held.node().as_str()))
        })
        .cloned()
        .collect();
    if members.is_empty() {
        return Ok(Vec::new());
    }
    let name = Medication::Orders
        .name()
        .map_err(|_unnamed| Unprescribed::Internal("a medication query has no qualified name"))?;
    let queried = queried(
        federation,
        arrived,
        located,
        (patient, (&name, Medication::Orders.aql())),
        &members,
    )
    .await?;
    let mut asked = Vec::with_capacity(members.len());
    for (endpoint, answered) in &queried {
        let (chosen, mode, system_id) = member_of(snapshot, targets, mapped, endpoint)?;
        let (held, composed) = match answered {
            Answered::Rows(held) => paged(held, (mode, &system_id), (patient, now)),
            Answered::Silent { status, left } => (
                Held::Unanswered {
                    status: *status,
                    left: *left,
                },
                Vec::new(),
            ),
        };
        asked.push(Member {
            chosen,
            held,
            composed: Some(composed),
        });
    }
    Ok(asked)
}

/// What one map-mode member answered a reserved medication query.
#[derive(Debug)]
pub(crate) enum Answered {
    /// The compositions it answered with, each with its uid and template id.
    Rows(Vec<ferrofed_eehrxf::summary::Held>),
    /// No answer, with its §11.1 status and whether a request left the
    /// gateway for it.
    Silent {
        /// Its §11.1 status.
        status: EndpointStatus,
        /// Whether a request left the gateway for it.
        left: bool,
    },
}

/// Runs the reserved query `name`, of the text `aql`, for the patient `patient`
/// over the members `members`, Query API endpoints the plan of `located`
/// dispatches to, as `arrived` asks, and returns what each answered, in
/// endpoint order.
///
/// # Errors
///
/// The [`Unprescribed`] of a query the gateway could not run.
pub(crate) async fn queried(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    located: &Located,
    (patient, (name, aql)): ((&str, &str), (&QualifiedQueryName, String)),
    members: &BTreeSet<EndpointId>,
) -> Result<Vec<(EndpointId, Answered)>, Unprescribed> {
    let snapshot = federation.snapshot();
    let logged = arrived.outbound.to_string();
    let analysis = located::stored(
        federation,
        located.completion,
        patient,
        (name, aql, &logged),
    )?;
    let (plan, sources) = plan::retarget(snapshot, &located.targets, patient_query(&analysis)?)
        .map_err(|failure| Unprescribed::Failure(Failure::Plan(failure)))?;
    let mut plan = answer::planning::planned(
        located.disclosed,
        plan,
        &analysis,
        (located.completion, DedupMode::None, analysis.attributes()),
    );
    let others: Vec<EndpointId> = plan
        .dispatched()
        .filter(|endpoint| !members.contains(*endpoint))
        .cloned()
        .collect();
    for endpoint in &others {
        plan = plan.leaving_out(endpoint);
    }
    let rows: Arc<Mutex<BTreeMap<EndpointId, Vec<ResultSetRow>>>> = Arc::default();
    let read = Arc::clone(&rows);
    let plan = plan.reading(RowReader::new(move |endpoint, answered| {
        read.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(endpoint.clone(), answered.to_vec());
    }));
    let answer = answer::fan_out::fanned_out(
        federation,
        plan,
        federation.budget(),
        (arrived.started, arrived.conveyance, arrived.outbound),
    )
    .await
    .map_err(|failure| Unprescribed::Failure(Failure::FanOut(failure)))?;
    answer::fan_out::observed(federation, &answer);
    follow_up::observe(federation, answer.seen(), &logged);
    let held = rows.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let left: BTreeSet<&EndpointId> = answer
        .contacts()
        .filter(|(_, contact)| contact.left())
        .map(|(endpoint, _)| endpoint)
        .collect();
    Ok(members
        .iter()
        .map(|endpoint| {
            let status = answer
                .federation()
                .endpoints()
                .iter()
                .find(|record| record.id().as_str() == endpoint.as_str())
                .map_or(EndpointStatus::Excluded, EndpointOutcome::status);
            let answered = if status == EndpointStatus::Active {
                let rows = held.get(endpoint).map_or(&[][..], Vec::as_slice);
                Answered::Rows(compositions(rows, &sources))
            } else {
                Answered::Silent {
                    status,
                    left: left.contains(endpoint),
                }
            };
            (endpoint.clone(), answered)
        })
        .collect())
}

/// The map-mode member behind `endpoint` as the search reaches it, with its
/// mapping among `mapped` and its `system_id`, its `ehr_id` the one `targets`
/// resolved.
fn member_of<'m>(
    snapshot: &RegistrySnapshot,
    targets: &plan::Targets,
    mapped: &'m BTreeMap<String, MapMode>,
    endpoint: &EndpointId,
) -> Result<(Chosen, &'m MapMode, SystemId), Unprescribed> {
    let Some(node) = snapshot.endpoint(endpoint).map(|held| held.node().clone()) else {
        return Err(Unprescribed::Internal(
            "a map-mode endpoint is one the registry does not hold",
        ));
    };
    let ehr_id = targets
        .resolved
        .iter()
        .find(|(resolved, _)| resolved == &node)
        .map(|(_, ehr_id)| ehr_id.clone())
        .ok_or(Unprescribed::Internal(
            "a map-mode member was asked with no ehr_id",
        ))?;
    let (Some(mode), Some(system_id)) = (
        mapped.get(node.as_str()),
        snapshot.node(&node).map(|held| held.system_id().clone()),
    ) else {
        return Err(Unprescribed::Internal(
            "a map-mode member has no mapping or no system_id",
        ));
    };
    let chosen = Chosen {
        via: endpoint.clone(),
        node,
        native: endpoint.clone(),
        ehr_id,
    };
    Ok((chosen, mode, system_id))
}

/// The pages the member owning the `system_id` answered in `rows`, each
/// composition's read from the columns `sources` places, mapped by `mode`
/// for the patient `patient` at `now`, with the UUID of each page's
/// versioned composition.
fn paged(
    held: &[ferrofed_eehrxf::summary::Held],
    (mode, system_id): (&MapMode, &SystemId),
    (patient, now): ((&str, &str), &str),
) -> (Held, Vec<Composed>) {
    let mut pages: Vec<Page> = Vec::new();
    let mut composed = Vec::new();
    for held in held {
        // NOTE: A102 point 1, BASE OBJECT_VERSION_ID: a uid that names no version, or a version
        // another system created, is no order the member owns, so it is legitimately not listed.
        let Ok(version) = ObjectVersionId::new(&held.uid) else {
            continue;
        };
        let owned =
            SystemId::creating_system_id_of(&version).is_ok_and(|creating| &creating == system_id);
        if !owned {
            continue;
        }
        match mode.page(&held.composition, patient, now) {
            Ok(page) => {
                composed.push(Composed {
                    object: Uuid::parse_str(&version.object_id().value()).ok(),
                    version: held.uid.clone(),
                    composition: held.composition.clone(),
                });
                pages.push(page);
            }
            Err(reason) => {
                return (
                    Held::Unusable {
                        status: StatusCode::OK,
                        reason: Unread::Mapped(reason),
                    },
                    Vec::new(),
                );
            }
        }
    }
    (Held::Found(pages), composed)
}

/// What a map-mode member answered the read of one of its prescriptions.
#[derive(Debug)]
pub(crate) enum Fetched {
    /// The page of the one prescription the id names, renamed by the
    /// composition's UUID `resource` the read compares.
    Held(Held),
    /// The identity binding names no patient for the member's `ehr_id`, so
    /// the mapped prescription could have no subject.
    Unnamed,
}

/// Reads the prescription at position `entry` among those the composition
/// of the versioned object `composition` maps to, at the map-mode member
/// `chosen` by its own `ehr_id`, mapped by `mode` at `now`, its patient
/// named in one of `namespaces` by the identity binding.
///
/// The composition is read by its versioned object uid (ITS-REST 1.1.0 EHR
/// API, `GET /ehr/{ehr_id}/composition/{uid_based_id}`), and it is the
/// member's own order only when its uid's `creating_system_id` is the
/// member's `system_id` and it has no `feeder_audit` (A102 point 1); any
/// other composition, or none, holds no prescription for the id.
pub(crate) async fn read(
    federation: &Federation,
    arrived: &Arrived<'_>,
    (chosen, mode, namespaces): (&Chosen, &MapMode, &[IdentifierNamespace]),
    (composition, entry, now): (&str, u8, &str),
) -> Result<Fetched, Failure> {
    let budget = federation.budget();
    let overall = arrived
        .started
        .checked_add(budget.overall())
        .ok_or(Failure::FanOut(FanOutError::Clock))?;
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    let Some(patient) = named(federation, arrived, chosen, (namespaces, per_node)).await else {
        return Ok(Fetched::Unnamed);
    };
    let body = match composition_at(federation, arrived, chosen, (composition, per_node)).await? {
        Composition::Body(body) => body,
        Composition::Absent => return Ok(Fetched::Held(Held::Found(Vec::new()))),
        Composition::Silent { status, left } => {
            return Ok(Fetched::Held(Held::Unanswered { status, left }));
        }
    };
    let snapshot = federation.snapshot();
    let Some(system_id) = snapshot.node(&chosen.node).map(Node::system_id) else {
        return Ok(Fetched::Held(Held::Found(Vec::new())));
    };
    Ok(Fetched::Held(picked(
        &body,
        (mode, system_id),
        (&patient, composition, entry, now),
    )))
}

/// What a member answered the read of one composition.
#[derive(Debug)]
pub(crate) enum Composition {
    /// The composition, as the member sent it.
    Body(Vec<u8>),
    /// The member holds no such composition for the `ehr_id`.
    Absent,
    /// No answer the read can take, with the member's §11.1 status and
    /// whether a request left the gateway for it.
    Silent {
        /// Its §11.1 status.
        status: EndpointStatus,
        /// Whether a request left the gateway for it.
        left: bool,
    },
}

/// Reads the composition of the versioned object or version `uid` at the
/// member `chosen`, by its own `ehr_id` alone, by the deadline `deadline`
/// (ITS-REST 1.1.0 EHR API, `GET /ehr/{ehr_id}/composition/{uid_based_id}`).
///
/// # Errors
///
/// [`Failure::FanOut`] when the member has no client.
pub(crate) async fn composition_at(
    federation: &Federation,
    arrived: &Arrived<'_>,
    chosen: &Chosen,
    (uid, deadline): (&str, Instant),
) -> Result<Composition, Failure> {
    let Some(client) = federation.clients().get(&chosen.via) else {
        return Err(Failure::FanOut(FanOutError::UnknownEndpoint {
            endpoint: chosen.via.clone(),
        }));
    };
    // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond what
    // it reads of every request, and the member is located by its ehr_id alone.
    let options = DispatchOptions::new(deadline, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(Arc::new(Withheld::default()));
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    let request = ClientRequest {
        method: Method::GET,
        path: format!(
            "/ehr/{}/composition/{}",
            path_segment(&chosen.ehr_id.as_str()),
            path_segment(&uid)
        ),
        query: None,
        headers,
        body: Vec::new(),
    };
    let answer = match client.forward(request, &options).await {
        Ok(answer) => answer,
        Err(failure) => {
            return Ok(Composition::Silent {
                status: unanswered(&failure),
                left: Contact::of_forward_error(&failure).left(),
            });
        }
    };
    Ok(match answer.status() {
        StatusCode::OK => Composition::Body(answer.body().to_vec()),
        StatusCode::NOT_FOUND => Composition::Absent,
        _ => Composition::Silent {
            status: EndpointStatus::NodeError,
            left: true,
        },
    })
}

/// The page of the one prescription at `entry` the composition `body`
/// maps to by `mode`, for `patient` at `now`, when the composition is the
/// own order of the member owning `system_id`, renamed by `composition`.
fn picked(
    body: &[u8],
    (mode, system_id): (&MapMode, &SystemId),
    ((system, value), composition, entry, now): (&(String, String), &str, u8, &str),
) -> Held {
    let unusable = |reason| Held::Unusable {
        status: StatusCode::OK,
        reason,
    };
    let Ok(read) = serde_json::from_slice::<OwnOrder>(body) else {
        return unusable(Unread::Other);
    };
    let owned = read
        .uid
        .and_then(|uid| ObjectVersionId::new(&uid.value).ok())
        .and_then(|version| SystemId::creating_system_id_of(&version).ok())
        .is_some_and(|creating| &creating == system_id);
    // NOTE: A102 point 1, RM 1.1.0 LOCATABLE.feeder_audit: a version another system created,
    // or an imported copy, is no order the member owns, so the id names nothing there.
    if !owned || read.feeder_audit.is_some() {
        return Held::Found(Vec::new());
    }
    let text = String::from_utf8_lossy(body);
    let mut page = match mode.page(&text, (system, value), now) {
        Ok(page) => page,
        Err(reason) => return unusable(Unread::Mapped(reason)),
    };
    let Some(mut prescription) = page.prescriptions.drain(..).nth(usize::from(entry)) else {
        return Held::Found(Vec::new());
    };
    prescription.kept.rename(composition);
    page.prescriptions = vec![prescription];
    Held::Found(vec![page])
}

/// What a composition is read for before it is mapped: its uid and whether
/// it carries a `feeder_audit`.
#[derive(serde::Deserialize)]
struct OwnOrder {
    /// The `uid`.
    uid: Option<Uid>,
    /// The `feeder_audit`, kept as it is.
    feeder_audit: Option<serde::de::IgnoredAny>,
}

/// The value of a composition's `uid`.
#[derive(serde::Deserialize)]
struct Uid {
    /// Its `value`.
    value: String,
}

/// The §11.1 status a forwarded request that got no answer shows.
fn unanswered(failure: &ForwardError) -> EndpointStatus {
    match failure {
        ForwardError::TimeOut { .. } | ForwardError::Expired(_) => EndpointStatus::TimeOut,
        ForwardError::Unreachable { .. } | ForwardError::Capped(_) => EndpointStatus::Offline,
        _ => EndpointStatus::NodeError,
    }
}

/// The patient behind `chosen`'s `ehr_id`, as the identity binding names
/// it in the first of `namespaces` it holds an identifier in, by the
/// deadline `deadline`.
pub(crate) async fn named(
    federation: &Federation,
    arrived: &Arrived<'_>,
    chosen: &Chosen,
    (namespaces, deadline): (&[IdentifierNamespace], Instant),
) -> Option<(String, String)> {
    let resolver = federation.resolver()?;
    if namespaces.is_empty() {
        return None;
    }
    let named = resolver
        .identify(
            &chosen.node,
            &chosen.ehr_id,
            namespaces,
            arrived.on_behalf,
            deadline,
        )
        .await;
    let Identification::Named(patients) = named else {
        return None;
    };
    let patient = patients.first()?;
    Some((
        patient.namespace().as_str().to_owned(),
        patient.withheld().expose_secret().to_owned(),
    ))
}
