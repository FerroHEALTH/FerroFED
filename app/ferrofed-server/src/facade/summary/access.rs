// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access record of a gathered summary, list or lookup (Regulation (EU)
//! 2025/327 Annex II 3.2): the ids each section query constrained its data
//! to, the members it reached and where the patient resolved.

use std::collections::BTreeSet;
use std::sync::Arc;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence, Queried, ResourceKind, RootObject};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier, PatientLookup};
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aql::Analysis;
use openehr_federation::status::EndpointStatus;
use secrecy::SecretString;

use super::{Gathered, LIST, LOOKUP, OPERATION, Scope};
use crate::access::{Accessed, Asked};
use crate::facade::mapped::Answered;
use crate::facade::native::{self};
use crate::federation::Federation;

/// The ids every section query constrains its data to.
pub(super) fn queried(analyses: &[(Section, Analysis)]) -> Queried {
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
    // NOTE: Annex II 3.2 logs "every access event"; a list that lists the gateway's summary, or a
    // lookup that finds the patient, discloses that members hold the patient's record, health
    // data (Regulation (EU) 2016/679 recital 35).
    let listed = scope != Scope::Summary && !gathered.resolved.is_empty();
    if gathered.sent.is_empty() && gathered.set_aside.is_empty() && !listed {
        return None;
    }
    let evidence = |objects: Vec<RootObject>| match scope {
        Scope::Summary => Evidence::reached(Basis::Returned, objects)
            .queried(gathered.queried.clone())
            .constructed(Category::PatientSummary),
        // NOTE: Regulation (EU) 2025/327 Art 14(1)(e) and (f): a list that asked for laboratory
        // or discharge reports reaches that category too.
        Scope::List => listed_categories(gathered).into_iter().fold(
            Evidence::reached(Basis::Returned, objects),
            Evidence::constructed,
        ),
        // NOTE: openEHR RM EHR; a lookup reaches the fact that the patient's EHRs exist, and no
        // composition, so it holds data of no category.
        Scope::Lookup => Evidence::resource(ResourceKind::Ehr),
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
    origins.extend(laboratory_origins(federation.snapshot(), gathered));
    origins.extend(own_origins(
        federation.snapshot(),
        gathered,
        natives_delivered,
    ));
    if listed {
        origins.extend(resolved_origins(federation.snapshot(), gathered));
    }
    let (operation, query) = match scope {
        Scope::Summary => (OPERATION, format!("identifier={system}|{value}")),
        Scope::List => (LIST, format!("patient.identifier={system}|{value}")),
        Scope::Lookup => (LOOKUP, format!("identifier={system}|{value}")),
    };
    let documents = documents(gathered, natives_delivered);
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Query,
        operation,
        resource: None,
        // NOTE: BALP 1.1.4 entity:query holds the request, which a FHIR operation or search
        // names in its parameters.
        query: Some(SecretString::from(query)),
        stored_query: None,
        subject: if listed {
            listed_subject(federation.snapshot(), gathered, (system, value))
        } else {
            subject(federation.snapshot(), gathered, (system, value))
        },
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

/// Each member a list asked the laboratory report query of, with what it
/// answered: an origin that contributed when it holds a laboratory report.
fn laboratory_origins(snapshot: &RegistrySnapshot, gathered: &Gathered) -> Vec<Asked> {
    gathered
        .laboratory
        .iter()
        .filter_map(|(endpoint, answered)| {
            let (status, rows) = match answered {
                Answered::Rows(held) => (EndpointStatus::Active, held.len()),
                Answered::Silent { status, left: true } => (*status, 0),
                Answered::Silent { .. } => return None,
            };
            let node = node_of(snapshot, endpoint)?;
            Some(Asked {
                endpoint: endpoint.as_str().to_owned(),
                node: Some(node.as_str().to_owned()),
                system_id: snapshot
                    .node(&node)
                    .map(|held| held.system_id().as_str().to_owned()),
                status: status.as_str().to_owned(),
                rows: Some(rows),
                version: None,
                created_ehr: None,
                contributed: rows > 0,
                evidence: (rows > 0).then(|| {
                    Evidence::reached(Basis::Returned, Vec::new())
                        .constructed(Category::MedicalTestResult)
                }),
            })
        })
        .collect()
}

/// The categories a list `gathered` holds documents of: the patient summary,
/// and each category it asked a member for.
fn listed_categories(gathered: &Gathered) -> BTreeSet<Category> {
    let mut categories = BTreeSet::from([Category::PatientSummary]);
    if !gathered.laboratory.is_empty() {
        categories.insert(Category::MedicalTestResult);
    }
    categories.extend(gathered.own.iter().map(|own| own.kind.category()));
    categories
}

/// The number of documents the members of `gathered` serve themselves that
/// a list `delivered`: each member's own summary and each of its own
/// laboratory and discharge reports, none when it delivered nothing.
fn documents(gathered: &Gathered, delivered: bool) -> usize {
    if !delivered {
        return 0;
    }
    let summaries = gathered
        .natives
        .iter()
        .filter(|native| {
            matches!(
                native.answer,
                ferrofed_engine::single_node::native::Answer::Document(_)
            )
        })
        .count();
    gathered
        .own
        .iter()
        .map(|own| own.held.prescriptions())
        .fold(summaries, usize::saturating_add)
}

/// Each member a list asked at its own endpoint for its laboratory or
/// discharge reports whose request left the gateway, with what it answered: with `delivered`,
/// an origin that contributed when the list carries one of its documents.
fn own_origins(snapshot: &RegistrySnapshot, gathered: &Gathered, delivered: bool) -> Vec<Asked> {
    gathered
        .own
        .iter()
        .filter(|own| own.held.left())
        .map(|own| {
            let documents = own.held.prescriptions();
            let contributed = delivered && documents > 0;
            Asked {
                endpoint: own.chosen.native.as_str().to_owned(),
                node: Some(own.chosen.node.as_str().to_owned()),
                system_id: snapshot
                    .node(&own.chosen.node)
                    .map(|held| held.system_id().as_str().to_owned()),
                status: own.held.status().as_str().to_owned(),
                rows: Some(documents),
                version: None,
                created_ehr: None,
                contributed,
                evidence: contributed.then(|| {
                    Evidence::reached(Basis::Returned, Vec::new()).constructed(own.kind.category())
                }),
            }
        })
        .collect()
}

/// The members a list's patient resolved at that no request of the list left
/// for, each at the endpoint it is asked through: the origins of the
/// gateway's summary the list names, which none of them was asked for.
pub(super) fn resolved_origins(snapshot: &RegistrySnapshot, gathered: &Gathered) -> Vec<Asked> {
    gathered
        .resolved
        .iter()
        .filter(|(node, _)| {
            !gathered
                .sent
                .iter()
                .any(|sent| node_of(snapshot, sent).as_ref() == Some(node))
        })
        .filter_map(|(node, _)| {
            let endpoint = snapshot.asked_through(node)?;
            Some(Asked {
                endpoint: endpoint.id().as_str().to_owned(),
                node: Some(node.as_str().to_owned()),
                system_id: snapshot
                    .node(node)
                    .map(|held| held.system_id().as_str().to_owned()),
                status: EndpointStatus::Active.as_str().to_owned(),
                rows: None,
                version: None,
                created_ehr: None,
                contributed: false,
                evidence: None,
            })
        })
        .collect()
}

/// The member `endpoint` belongs to, a Query API endpoint or a member's own
/// exchange-format endpoint.
pub(super) fn node_of(snapshot: &RegistrySnapshot, endpoint: &EndpointId) -> Option<NodeId> {
    snapshot
        .endpoint(endpoint)
        .map(|held| held.node().clone())
        .or_else(|| {
            snapshot
                .native_endpoint(endpoint)
                .map(|native| native.node().clone())
        })
}

/// Whose data a list reached: the patient `system`|`value` and the `ehr_id`
/// it resolved to at each member, at the endpoint a request left for or, for
/// a member the list asked nothing, the endpoint it is asked through.
pub(super) fn listed_subject(
    snapshot: &RegistrySnapshot,
    gathered: &Gathered,
    patient: (&str, &str),
) -> DataSubject {
    let mut named = subject(snapshot, gathered, patient);
    for (node, ehr_id) in &gathered.resolved {
        let held = named.ehrs.iter().any(|ehr| {
            ehr.ehr_id == ehr_id.as_str()
                && EndpointId::new(ehr.endpoint.as_str())
                    .ok()
                    .and_then(|endpoint| node_of(snapshot, &endpoint))
                    .is_some_and(|at| &at == node)
        });
        if held {
            continue;
        }
        if let Some(endpoint) = snapshot.asked_through(node) {
            named.ehrs.push(EhrAt {
                endpoint: endpoint.id().as_str().to_owned(),
                ehr_id: ehr_id.as_str().to_owned(),
                patient: PatientLookup::RequestNamed,
            });
        }
    }
    named
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
            namespace: Some(system.to_owned()),
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
            .chain(
                gathered
                    .own
                    .iter()
                    .filter(|own| own.held.left())
                    .map(|own| EhrAt {
                        endpoint: own.chosen.native.as_str().to_owned(),
                        ehr_id: own.chosen.ehr_id.as_str().to_owned(),
                        patient: PatientLookup::RequestNamed,
                    }),
            )
            .collect(),
        population: None,
    }
}
