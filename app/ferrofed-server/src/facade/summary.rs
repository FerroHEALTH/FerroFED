// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary's section queries, run over the members as one
//! request for the FHIR face (Regulation (EU) 2025/327 Annex II 2.1).
//!
//! Each section query is the gateway's own stored query
//! ([`ferrofed_eehrxf::patient_summary`]), the text the stored-query registry
//! lists under the reserved namespace at its one version (§12.7, N44), read
//! here from the code that defines it, so the face runs with or without
//! `[stored_queries]`. Every query is read and analysed as the ITS-REST
//! façade reads a stored query (§7.1), with the patient bound through
//! `$patient` and `$namespace` alone. The patient is localized, checked
//! against the consent pre-filter and resolved once, for the first query,
//! and every other query is planned over that resolution
//! ([`plan::retarget`]), so the identity services are asked once per
//! summary and every query reaches the same `{node, ehr_id}` pairs. Every
//! query passes the rewrite and the outbound identifier-hygiene gate, so no
//! patient identifier reaches a node (§5.4.1, N33). The queries are sent
//! together under one budget (§11.5), and the summary is complete only when
//! every one is: under all-or-nothing, a member that does not answer one of
//! them fails the whole summary, naming it (§11.3, §11.4).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence, Queried, RootObject};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup};
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::{
    Answer, Covered, Elsewhere, Held, Organisation, Origin, SectionAnswers,
};
use ferrofed_engine::fanout::reader::RowReader;
use ferrofed_engine::fanout::{FanOutError, FederatedAnswer, Plan};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::StatusCode;
use openehr_federation::aql::{Analysis, ColumnSource, PatientQuery};
use openehr_federation::dedup::DedupMode;
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::generated::query::ResultSetRow;
use secrecy::SecretString;
use tokio::task::JoinSet;

use crate::access::{Accessed, Asked};
use crate::facade::answer::{self, Failure};
use crate::facade::located::{Located, located};
use crate::facade::native::{self, Native};
use crate::facade::request::Arrived;
use crate::facade::{cells, follow_up, plan, security};
use crate::federation::Federation;

/// The operation an access record names for a patient summary.
pub(crate) const OPERATION: &str = "Patient/$summary";

/// The operation an access record names for the document list.
pub(crate) const LIST: &str = "DocumentReference";

/// The rows each endpoint answered one section query with, read before the
/// merge.
type Rows = Arc<Mutex<BTreeMap<EndpointId, Vec<ResultSetRow>>>>;

/// One planned section query: its section, its plan, the column sources of
/// its node rows, and where the rows each endpoint answered are kept.
type Planned = (Section, Plan, Vec<ColumnSource>, Rows);

/// One answered section query.
type Answered = (Section, FederatedAnswer, Vec<ColumnSource>, Rows);

/// One member endpoint the section queries were dispatched to, and what it
/// showed.
#[derive(Debug, Clone)]
pub(crate) struct Reached {
    /// The endpoint.
    pub(crate) endpoint: EndpointId,
    /// The member behind it.
    pub(crate) node: NodeId,
    /// The member's `system_id`, as the answer named it.
    pub(crate) system_id: Option<String>,
    /// Its status: `active` when it answered every query, and otherwise the
    /// status of the first query it did not answer (§11.1).
    pub(crate) status: EndpointStatus,
    /// The compositions it answered with, over every section.
    pub(crate) compositions: usize,
    /// The archetype roots of what it answered with, for the access log.
    pub(crate) objects: Vec<RootObject>,
}

/// What the section queries gathered from the members.
#[derive(Debug)]
pub(crate) struct Gathered {
    /// Each member endpoint the queries were dispatched to, in endpoint
    /// order: the origins of the summary.
    pub(crate) reached: Vec<Reached>,
    /// The origins as the document names them, in the same order.
    pub(crate) origins: Vec<Origin>,
    /// Per section, what each origin answered.
    pub(crate) sections: Vec<SectionAnswers>,
    /// The `{node, ehr_id}` pairs the patient resolved to.
    pub(crate) resolved: Vec<(NodeId, EhrId)>,
    /// The endpoints a request left the gateway for.
    pub(crate) sent: BTreeSet<EndpointId>,
    /// The ids the section queries constrain their data to.
    pub(crate) queried: Queried,
    /// The members whose consent pre-filter denial was set aside for an
    /// emergency purpose.
    pub(crate) set_aside: BTreeSet<NodeId>,
    /// The members that serve the patient summary themselves, each with
    /// what its own endpoint answered, in endpoint order.
    pub(crate) natives: Vec<Native>,
    /// The Query API endpoint of each member the resolver could not answer
    /// for, in endpoint order.
    pub(crate) unresolved: Vec<EndpointId>,
}

/// Why the section queries give no summary.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Unsummarised {
    /// The request or a query was refused before anything was sent, or the
    /// fan-out failed on the gateway's side.
    #[error(transparent)]
    Failure(#[from] Failure),
    /// The gateway could not run the section queries, for a reason of its
    /// own that names no value.
    #[error("the section queries could not be run: {0}")]
    Internal(&'static str),
    /// No member holds the patient, or none may say so (§11.3), with what
    /// was gathered when a consent denial was set aside, which its access
    /// record names.
    #[error("no member holds a patient summary for this identifier")]
    NotFound(Option<Box<Gathered>>),
    /// Under all-or-nothing, a member did not answer a section query
    /// (§11.3, §11.4); the answer names each such member.
    #[error("the summary is incomplete: {}", named(silent))]
    Incomplete {
        /// The status the answer takes.
        status: StatusCode,
        /// Each endpoint that did not answer, with its §11.1 status.
        silent: Vec<(String, String)>,
        /// What was gathered, for the access record of the requests that
        /// left the gateway.
        gathered: Box<Gathered>,
    },
}

impl Unsummarised {
    /// Whether node selection left no member in scope for the patient, as
    /// for a patient no member holds (§11.2, §11.3).
    pub(crate) fn reaches_no_member(&self) -> bool {
        matches!(self, Self::Failure(Failure::NoDestination))
    }
}

/// The silent members `silent` names, for a message.
fn named(silent: &[(String, String)]) -> String {
    silent
        .iter()
        .map(|(endpoint, status)| format!("{endpoint} ({status})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a request gathers from the members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The patient summary: every section query, and every member that
    /// serves the summary itself.
    Summary,
    /// The document list: the members that serve the summary themselves
    /// alone, since the summary the gateway writes is listed, never written.
    List,
}

/// Runs every section query over the members for the patient
/// `system`|`value`, as `arrived` asks, or, for `scope` [`Scope::List`],
/// none of them.
///
/// # Errors
///
/// The [`Unsummarised`] that says why no summary can be written.
pub(crate) async fn gather(
    federation: Arc<Federation>,
    arrived: Arrived<'_>,
    (system, value): (&str, &str),
    scope: Scope,
) -> Result<Gathered, Unsummarised> {
    let logged = arrived.outbound.to_string();
    let Located {
        completion,
        mut targets,
        analyses,
        disclosed,
    } = located(&federation, &arrived, (system, value)).await?;
    let dispatched = targets.plan.dispatched().next().is_some();
    // NOTE: § The four identifiers, N33: a member asked at its own exchange-format endpoint by
    // its ehr_id is asked no section query, so its data are never in the mapped document too.
    let chosen = native::chosen(federation.snapshot(), &targets);
    for member in &chosen {
        targets.plan = targets.plan.leaving_out(&member.via);
    }
    let withheld = targets.plan.withheld();
    let mut planned = Vec::with_capacity(analyses.len());
    let run: &[(Section, Analysis)] = if scope == Scope::Summary {
        &analyses
    } else {
        &[]
    };
    for (position, (section, analysis)) in run.iter().enumerate() {
        let (plan, sources) = if position == 0 {
            (targets.plan.clone(), targets.sources.clone())
        } else {
            plan::retarget(federation.snapshot(), &targets, patient_query(analysis)?)
                .map_err(Failure::Plan)?
        };
        let plan = answer::planning::planned(
            disclosed,
            plan,
            analysis,
            (completion, DedupMode::None, analysis.attributes()),
        );
        let rows: Rows = Arc::default();
        let read = Arc::clone(&rows);
        let plan = plan.reading(RowReader::new(move |endpoint, answered| {
            read.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(endpoint.clone(), answered.to_vec());
        }));
        planned.push((*section, plan, sources, rows));
    }
    let (answers, natives) = tokio::join!(
        fanned_out(&federation, &arrived, planned),
        native::asked(&federation, &arrived, chosen, withheld)
    );
    let (answers, natives) = (answers?, natives?);
    let mut status = StatusCode::OK;
    for (_, answer, _, _) in &answers {
        answer::fan_out::observed(&federation, answer);
        follow_up::observe(&federation, answer.seen(), &logged);
        let settled =
            answer::fan_out::settled(answer.status(), targets.resolution_failed, completion);
        if status == StatusCode::OK {
            status = settled;
        }
    }
    // NOTE: no specification governs this (§11.3 covers only an answered lookup): our own
    // design; a list asks no section query, so its failed resolution is settled here.
    let status = answer::fan_out::settled(status, targets.resolution_failed, completion);
    let status = native::settled(&natives, status, completion);
    let mut gathered = collected(&federation, &targets.resolved, &answers, &analyses)?;
    gathered.set_aside = targets.set_aside;
    gathered.unresolved = targets.unresolved;
    for native in &natives {
        if native.answer.contact().left() {
            gathered.sent.insert(native.chosen.native.clone());
        }
    }
    gathered.natives = natives;
    concluded(gathered, status, (scope, dispatched))
}

/// The summary `gathered` gives, the section queries and the members' own
/// endpoints having settled on `status`: every silent member named when it
/// is not `200`, and no summary when no member holds the patient (§11.3).
/// A list, which asks no section query, holds the summary the gateway
/// writes when the plan `dispatched` to a member.
fn concluded(
    gathered: Gathered,
    status: StatusCode,
    (scope, dispatched): (Scope, bool),
) -> Result<Gathered, Unsummarised> {
    if status != StatusCode::OK {
        let queried = gathered
            .reached
            .iter()
            .map(|reached| (&reached.endpoint, reached.status));
        let asked = gathered
            .natives
            .iter()
            .map(|native| (&native.chosen.native, native.answer.status()));
        let mut silent: Vec<(String, String)> = queried
            .chain(asked)
            .filter(|(_, status)| *status != EndpointStatus::Active)
            .map(|(endpoint, status)| (endpoint.as_str().to_owned(), status.as_str().to_owned()))
            .collect();
        for endpoint in &gathered.unresolved {
            if !silent.iter().any(|(named, _)| named == endpoint.as_str()) {
                silent.push((
                    endpoint.as_str().to_owned(),
                    EndpointStatus::NotResolved.as_str().to_owned(),
                ));
            }
        }
        return Err(Unsummarised::Incomplete {
            status,
            silent,
            gathered: Box::new(gathered),
        });
    }
    let holds = match scope {
        Scope::Summary => !gathered.reached.is_empty() || !gathered.natives.is_empty(),
        Scope::List => dispatched,
    };
    if !holds {
        let set_aside = (!gathered.set_aside.is_empty()).then(|| Box::new(gathered));
        return Err(Unsummarised::NotFound(set_aside));
    }
    Ok(gathered)
}

/// The patient query of `analysis`, which every section query is.
fn patient_query(analysis: &Analysis) -> Result<&PatientQuery, Unsummarised> {
    match analysis {
        Analysis::Patient(query) => Ok(query),
        Analysis::Unscoped(_) => Err(Unsummarised::Internal("a section query names no patient")),
    }
}

/// Sends every `planned` query at once under the request's budget, and
/// returns the answers in section order.
async fn fanned_out(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    planned: Vec<Planned>,
) -> Result<Vec<Answered>, Failure> {
    let budget = federation.budget();
    let mut tasks = JoinSet::new();
    let mut kept = BTreeMap::new();
    for (position, (section, plan, sources, rows)) in planned.into_iter().enumerate() {
        kept.insert(position, (section, sources, rows));
        let federation = Arc::clone(federation);
        let conveyance = arrived.conveyance.clone();
        let (started, outbound) = (arrived.started, arrived.outbound);
        tasks.spawn(async move {
            let answer = answer::fan_out::fanned_out(
                &federation,
                plan,
                budget,
                (started, &conveyance, outbound),
            )
            .await;
            (position, answer)
        });
    }
    let logged = arrived.outbound.to_string();
    let mut answered = BTreeMap::new();
    while let Some(joined) = tasks.join_next().await {
        let (position, answer) =
            joined.map_err(|joined| Failure::FanOut(FanOutError::Task(joined)))?;
        let answer = answer.map_err(|error| {
            security::fan_out(&error, &logged);
            Failure::FanOut(error)
        })?;
        answered.insert(position, answer);
    }
    Ok(kept
        .into_iter()
        .filter_map(|(position, (section, sources, rows))| {
            answered
                .remove(&position)
                .map(|answer| (section, answer, sources, rows))
        })
        .collect())
}

/// What the `answers` gathered, over the `resolved` pairs.
fn collected(
    federation: &Federation,
    resolved: &[(NodeId, EhrId)],
    answers: &[Answered],
    analyses: &[(Section, Analysis)],
) -> Result<Gathered, Unsummarised> {
    let snapshot = federation.snapshot();
    let mut dispatched: BTreeMap<EndpointId, Reached> = BTreeMap::new();
    let mut sent = BTreeSet::new();
    for (_, answer, _, _) in answers {
        for (endpoint, contact) in answer.contacts() {
            if contact.left() {
                sent.insert(endpoint.clone());
            }
            let Some(node) = snapshot.endpoint(endpoint).map(|held| held.node().clone()) else {
                return Err(Unsummarised::Internal(
                    "an answer names an endpoint the registry does not hold",
                ));
            };
            let record = answer
                .federation()
                .endpoints()
                .iter()
                .find(|record| record.id().as_str() == endpoint.as_str());
            let reached = dispatched
                .entry(endpoint.clone())
                .or_insert_with(|| Reached {
                    endpoint: endpoint.clone(),
                    node,
                    system_id: None,
                    status: EndpointStatus::Active,
                    compositions: 0,
                    objects: Vec::new(),
                });
            if let Some(record) = record {
                if reached.system_id.is_none() {
                    reached.system_id = record.system_id().map(str::to_owned);
                }
                if reached.status == EndpointStatus::Active {
                    reached.status = record.status();
                }
            }
        }
    }
    let reached: Vec<Reached> = dispatched.into_values().collect();
    let origins = reached
        .iter()
        .map(|reached| Origin {
            endpoint: reached.endpoint.as_str().to_owned(),
            organisation: organisation(snapshot, &reached.node),
        })
        .collect();
    let mut gathered = Gathered {
        reached,
        origins,
        sections: Vec::with_capacity(answers.len()),
        resolved: resolved.to_vec(),
        sent,
        queried: queried(analyses),
        set_aside: BTreeSet::new(),
        natives: Vec::new(),
        unresolved: Vec::new(),
    };
    for (section, answer, sources, rows) in answers {
        let held = rows.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let mut section_answers = Vec::with_capacity(gathered.reached.len());
        for (index, reached) in gathered.reached.iter_mut().enumerate() {
            let status = answer
                .federation()
                .endpoints()
                .iter()
                .find(|record| record.id().as_str() == reached.endpoint.as_str())
                .map_or(EndpointStatus::Excluded, EndpointOutcome::status);
            if status != EndpointStatus::Active {
                section_answers.push((
                    index,
                    Answer::Silent {
                        status: status.as_str().to_owned(),
                    },
                ));
                continue;
            }
            let rows = held.get(&reached.endpoint).map_or(&[][..], Vec::as_slice);
            let compositions = compositions(rows, sources);
            reached.compositions = reached.compositions.saturating_add(compositions.len());
            reached
                .objects
                .extend(cells::node_root_objects(rows, sources).0);
            section_answers.push((index, Answer::Answered(compositions)));
        }
        gathered.sections.push(SectionAnswers {
            section: *section,
            answers: section_answers,
        });
    }
    Ok(gathered)
}

/// The compositions of `rows`, each read from the composition, uid and
/// template id columns `sources` places in a node row.
fn compositions(rows: &[ResultSetRow], sources: &[ColumnSource]) -> Vec<Held> {
    let node = |column: usize| match sources.get(column) {
        Some(ColumnSource::Node(index)) => Some(*index),
        _ => None,
    };
    let (Some(composition), Some(uid), Some(template)) = (node(0), node(1), node(2)) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            // NOTE: AQL §SELECT; a row without its composition, uid or template id holds
            // nothing a mapping can be chosen for, so it is legitimately no composition.
            Some(Held {
                composition: cells::json(row, composition)?,
                uid: cells::text(row, uid)?.to_owned(),
                template: cells::text(row, template)?.to_owned(),
            })
        })
        .collect()
}

/// The organisation that holds `node`'s data, as the registry names it.
pub(crate) fn organisation(snapshot: &RegistrySnapshot, node: &NodeId) -> Organisation {
    let held = snapshot
        .node(node)
        .and_then(|node| snapshot.organisation(node.organisation()));
    Organisation {
        id: held.map_or_else(
            || node.as_str().to_owned(),
            |held| held.id().as_str().to_owned(),
        ),
        name: held.and_then(|held| held.name().map(str::to_owned)),
        identifiers: held
            .map(|held| {
                held.identifiers()
                    .iter()
                    .map(|identifier| {
                        (
                            identifier.system().to_owned(),
                            identifier.value().to_owned(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// The members `gathered` asked at their own endpoint, as the document
/// names them.
pub(crate) fn elsewhere(snapshot: &RegistrySnapshot, gathered: &Gathered) -> Vec<Elsewhere> {
    gathered
        .natives
        .iter()
        .map(|native| Elsewhere {
            endpoint: native.chosen.native.as_str().to_owned(),
            organisation: organisation(snapshot, &native.chosen.node),
            answer: match &native.answer {
                ferrofed_engine::single_node::native::Answer::Document(_) => Covered::Document,
                ferrofed_engine::single_node::native::Answer::Absent => Covered::Absent,
                silent => Covered::Silent {
                    status: silent.status().as_str().to_owned(),
                },
            },
        })
        .collect()
}

/// The ids every section query constrains its data to.
fn queried(analyses: &[(Section, Analysis)]) -> Queried {
    let mut queried = Queried {
        every_root_bound: true,
        ..Queried::default()
    };
    for (_, analysis) in analyses {
        let constrained = analysis.constrained();
        queried
            .templates
            .extend(constrained.templates().iter().cloned());
        queried
            .archetypes
            .extend(constrained.archetypes().iter().cloned());
        queried.every_root_bound &= constrained.every_root_bound();
    }
    queried
}

/// The access record's facts of a request of `scope` for the patient
/// `system`|`value` that `gathered` shows, or `None` when no request left
/// the gateway or the federation keeps no access log (Annex II 3.2).
///
/// `delivered` holds the compositions the gateway's document delivered,
/// each by its origin's index and its uid, and is `None` when the answer
/// delivered nothing; a list that answered delivered every member's own
/// document it carries. The categories are read from every composition a
/// member answered with, and the request serves the patient summary
/// category by construction (Art 14(1)(a)), whatever the deployment's map
/// says of its templates.
pub(crate) fn accessed(
    federation: &Federation,
    gathered: &Gathered,
    (system, value): (&str, &str),
    (scope, delivered): (Scope, Option<&BTreeSet<(usize, String)>>),
) -> Option<Accessed> {
    let none = BTreeSet::new();
    let natives_delivered = scope == Scope::List && delivered.is_some();
    let delivered = delivered.unwrap_or(&none);
    let log = federation.access_log()?;
    if gathered.sent.is_empty() && gathered.set_aside.is_empty() {
        return None;
    }
    let evidence = |objects: Vec<RootObject>| {
        let reached = Evidence::reached(Basis::Returned, objects);
        match scope {
            Scope::Summary => reached.queried(gathered.queried.clone()),
            Scope::List => reached,
        }
        .constructed(Category::PatientSummary)
    };
    let objects = gathered
        .reached
        .iter()
        .flat_map(|reached| reached.objects.iter().cloned())
        .collect();
    let mut origins: Vec<Asked> = gathered
        .reached
        .iter()
        .enumerate()
        .filter(|(_, reached)| gathered.sent.contains(&reached.endpoint))
        .map(|(index, reached)| {
            let contributed = delivered.iter().any(|(origin, _)| *origin == index);
            Asked {
                endpoint: reached.endpoint.as_str().to_owned(),
                node: Some(reached.node.as_str().to_owned()),
                system_id: reached.system_id.clone(),
                status: reached.status.as_str().to_owned(),
                rows: Some(reached.compositions),
                version: None,
                created_ehr: None,
                contributed,
                evidence: contributed.then(|| evidence(reached.objects.clone())),
            }
        })
        .collect();
    origins.extend(native::origins(
        federation.snapshot(),
        &gathered.natives,
        natives_delivered,
    ));
    let (operation, query) = match scope {
        Scope::Summary => (OPERATION, format!("identifier={system}|{value}")),
        Scope::List => (LIST, format!("patient.identifier={system}|{value}")),
    };
    let documents = gathered
        .natives
        .iter()
        .filter(|native| {
            natives_delivered
                && matches!(
                    native.answer,
                    ferrofed_engine::single_node::native::Answer::Document(_)
                )
        })
        .count();
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Query,
        operation,
        resource: None,
        // NOTE: BALP 1.1.4 entity:query holds the request, which a FHIR operation or search
        // names in its parameters.
        query: Some(SecretString::from(query)),
        stored_query: None,
        subject: subject(federation.snapshot(), gathered, (system, value)),
        evidence: evidence(objects),
        delivered: Some(delivered.len().saturating_add(documents)),
        origins,
        consent_set_aside: gathered
            .set_aside
            .iter()
            .map(|member| member.as_str().to_owned())
            .collect(),
        inserted: None,
        transformations: Vec::new(),
    })
}

/// Whose data `gathered` reached: the patient `system`|`value` and the
/// `ehr_id` it resolved to at each member a request left the gateway for.
pub(crate) fn subject(
    snapshot: &RegistrySnapshot,
    gathered: &Gathered,
    (system, value): (&str, &str),
) -> DataSubject {
    DataSubject {
        patient: Some(PatientIdentifier {
            namespace: system.to_owned(),
            value: SecretString::from(value.to_owned()),
        }),
        ehrs: gathered
            .resolved
            .iter()
            .filter_map(|(node, ehr_id)| {
                let sent = gathered.sent.iter().find(|sent| {
                    snapshot
                        .endpoint(sent)
                        .is_some_and(|endpoint| endpoint.node() == node)
                })?;
                Some(EhrAt {
                    endpoint: sent.as_str().to_owned(),
                    ehr_id: ehr_id.as_str().to_owned(),
                    patient: PatientLookup::RequestNamed,
                })
            })
            .chain(native::ehrs(&gathered.natives))
            .collect(),
        population: None,
    }
}
