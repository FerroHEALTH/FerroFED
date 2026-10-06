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
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier};
use ferrofed_eehrxf::patient_summary::{ISSUER, PATIENT, Section};
use ferrofed_eehrxf::summary::{Answer, Held, Organisation, Origin, SectionAnswers};
use ferrofed_engine::fanout::reader::RowReader;
use ferrofed_engine::fanout::{Completion, FanOutError, FederatedAnswer, Plan};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, StatusCode};
use openehr_federation::aql::{Analysis, ColumnSource, PatientQuery};
use openehr_federation::dedup::DedupMode;
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::generated::query::{AdhocQueryExecute, ResultSetRow};
use secrecy::SecretString;
use tokio::task::JoinSet;

use crate::access::{Accessed, Asked};
use crate::facade::answer::{self, Failure};
use crate::facade::request::{self, Arrived, Submitted};
use crate::facade::{cells, completeness, follow_up, intake, plan, security};
use crate::federation::Federation;

/// The operation an access record names for a patient summary.
pub(crate) const OPERATION: &str = "Patient/$summary";

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
    /// No member holds the patient, or none may say so (§11.3).
    #[error("no member holds a patient summary for this identifier")]
    NotFound,
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

/// The silent members `silent` names, for a message.
fn named(silent: &[(String, String)]) -> String {
    silent
        .iter()
        .map(|(endpoint, status)| format!("{endpoint} ({status})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs every section query over the members for the patient
/// `system`|`value`, as `arrived` asks.
///
/// # Errors
///
/// The [`Unsummarised`] that says why no summary can be written.
pub(crate) async fn gather(
    federation: Arc<Federation>,
    arrived: Arrived<'_>,
    (system, value): (&str, &str),
) -> Result<Gathered, Unsummarised> {
    let logged = arrived.outbound.to_string();
    let completion =
        completeness::of(arrived.headers, federation.best_effort()).map_err(Failure::from)?;
    let deadline = arrived
        .started
        .checked_add(federation.budget().overall())
        .ok_or(Failure::FanOut(FanOutError::Clock))?;
    let analyses = analysed(&federation, completion, (system, value), &logged)?;
    let (_, first) = analyses
        .first()
        .ok_or(Unsummarised::Internal("no section query"))?;
    answer::confinement::confine(
        &federation,
        (arrived.conveyance, arrived.on_behalf),
        first,
        (deadline, &logged),
    )
    .await?;
    let disclosed = federation.discloses_consent_to(arrived.conveyance);
    let (targets, _) = answer::planning::targeted(
        &federation,
        (first, plan::Selection::Undirected),
        (arrived.requester, arrived.on_behalf),
        (deadline, disclosed),
    )
    .await?;
    answer::confinement::held_within(&federation, arrived.conveyance, (first, &targets), &logged)?;
    if targets.plan.has_no_destination() {
        return Err(Failure::NoDestination.into());
    }
    answer::planning::remember(&federation, arrived.session, &targets);
    let mut planned = Vec::with_capacity(analyses.len());
    for (position, (section, analysis)) in analyses.iter().enumerate() {
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
    let answers = fanned_out(&federation, &arrived, planned).await?;
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
    let gathered = collected(&federation, &targets.resolved, &answers, &analyses)?;
    if status != StatusCode::OK {
        let silent = gathered
            .reached
            .iter()
            .filter(|reached| reached.status != EndpointStatus::Active)
            .map(|reached| {
                (
                    reached.endpoint.as_str().to_owned(),
                    reached.status.as_str().to_owned(),
                )
            })
            .collect();
        return Err(Unsummarised::Incomplete {
            status,
            silent,
            gathered: Box::new(gathered),
        });
    }
    if gathered.reached.is_empty() {
        return Err(Unsummarised::NotFound);
    }
    Ok(gathered)
}

/// Every section query, read and analysed as a stored query bound to the
/// patient `system`|`value`, under `completion`.
fn analysed(
    federation: &Federation,
    completion: Completion,
    (system, value): (&str, &str),
    logged: &str,
) -> Result<Vec<(Section, Analysis)>, Unsummarised> {
    // NOTE: §8.4; the FHIR face names no endpoint, so no targeting header is read.
    let undirected = HeaderMap::new();
    let mut analyses = Vec::with_capacity(Section::ALL.len());
    for section in Section::ALL {
        let name = section
            .name()
            .map_err(|_unnamed| Unsummarised::Internal("a section query has no qualified name"))?;
        let request = AdhocQueryExecute {
            q: section.aql(),
            offset: None,
            fetch: None,
            query_parameters: Some(intake::strings([(PATIENT, value), (ISSUER, system)])),
            additional_properties: BTreeMap::new(),
        };
        let submitted = Submitted::Stored {
            request,
            name: name.as_str(),
        };
        let (_, _, parsed) = request::read(
            federation,
            (submitted, &undirected),
            (completion, DedupMode::None),
            logged,
        )?;
        analyses.push((section, parsed));
    }
    Ok(analyses)
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
fn organisation(snapshot: &RegistrySnapshot, node: &NodeId) -> Organisation {
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

/// The access record's facts of a summary of the patient `system`|`value`
/// that `gathered` shows, which delivered the compositions `delivered`, each
/// by its origin's index and its uid, or `None` when no request left the
/// gateway or the federation keeps no access log (Annex II 3.2).
///
/// The categories are read from every composition a member answered with,
/// and the request serves the patient summary category by construction
/// (Art 14(1)(a)), whatever the deployment's map says of its templates.
pub(crate) fn accessed(
    federation: &Federation,
    gathered: &Gathered,
    (system, value): (&str, &str),
    delivered: &BTreeSet<(usize, String)>,
) -> Option<Accessed> {
    let log = federation.access_log()?;
    if gathered.sent.is_empty() {
        return None;
    }
    let evidence = |objects: Vec<RootObject>| {
        Evidence::reached(Basis::Returned, objects)
            .queried(gathered.queried.clone())
            .constructed(Category::PatientSummary)
    };
    let objects = gathered
        .reached
        .iter()
        .flat_map(|reached| reached.objects.iter().cloned())
        .collect();
    let origins = gathered
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
                contributed,
                evidence: contributed.then(|| evidence(reached.objects.clone())),
            }
        })
        .collect();
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Query,
        operation: OPERATION,
        resource: None,
        // NOTE: BALP 1.1.4 entity:query holds the request, which a FHIR operation names in
        // its parameters.
        query: Some(SecretString::from(format!("identifier={system}|{value}"))),
        stored_query: None,
        subject: subject(federation.snapshot(), gathered, (system, value)),
        evidence: evidence(objects),
        delivered: Some(delivered.len()),
        origins,
    })
}

/// Whose data `gathered` reached: the patient `system`|`value` and the
/// `ehr_id` it resolved to at each member a request left the gateway for.
fn subject(
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
                })
            })
            .collect(),
    }
}
