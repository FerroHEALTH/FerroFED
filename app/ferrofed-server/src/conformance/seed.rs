// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The run's writes to the nodes.
//!
//! The run writes the synthetic patient's EHR at every member the
//! deployment's cross-reference names, the vendored template and one
//! vendored composition in each, and the spare EHRs with no subject.
//!
//! Every write goes to the node's own ITS-REST API through the endpoint's
//! node client, with its onward credentials and the outbound gate, the
//! patient's value withheld from the path, the query string and the headers
//! (§5.4.1, N33): `PUT {base}/v1/ehr/{ehr_id}`,
//! `POST {base}/v1/definition/template/adl1.4` and
//! `POST {base}/v1/ehr/{ehr_id}/composition`, as ITS-REST 1.1.0 names them.
//! The patient travels only in the `EHR_STATUS` body of its create, a write
//! body the gate never reads (§5.4 scope note).
//!
//! The run writes into no EHR it did not create: a node that already holds
//! the `ehr_id` the cross-reference names answers the create `409`, and the
//! run then seeds the patient nowhere, so it never reads an EHR it did not
//! write. It deletes nothing. No specification governs the seed: our own
//! design.

use std::sync::Arc;
use std::time::Instant;

use ferrofed_engine::dispatch::{DispatchOptions, NodeClient};
use ferrofed_engine::forward::{ClientRequest, ForwardError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::conveyance::Conveyance;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::resolver::Resolution;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::EndpointStatus;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use openehr_its::json::to_canonical_json;
use uuid::Uuid;

use crate::admission::subject::ehr_status;
use crate::chain;
use crate::conformance::fixture::{
    Fixture, Holding, Member, PatientError, SeedData, Spares, SyntheticPatient, TEMPLATE_ID,
};
use crate::conveyed::{self, Unconveyed};
use crate::federation::Federation;
use crate::onward::NodeTransport;

/// One thing the run wrote to a node, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The endpoint it was written through.
    pub endpoint: String,
    /// What was written: the kind and its identifier.
    pub what: String,
}

/// A write the run could not make, or a run that could not start seeding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SeedError {
    /// The gateway cannot convey itself to a node, so nothing is sent
    /// (§13.1, N24).
    #[error("the run cannot convey the gateway to the nodes")]
    Unconveyed(#[source] Unconveyed),
    /// The per-node budget added to the current instant passes the range of
    /// the clock.
    #[error("the per-node budget passes the range of the clock")]
    Clock,
    /// The patient is no patient reference.
    #[error(transparent)]
    Patient(#[from] PatientError),
    /// A node has no client in the federation.
    #[error("endpoint {0} has no node client")]
    NoClient(String),
    /// A write got no answer of the node's.
    #[error("{step} at endpoint {endpoint} got no answer")]
    Unanswered {
        /// The write, as method and path.
        step: String,
        /// The endpoint.
        endpoint: String,
        /// What the client reported.
        #[source]
        source: ForwardError,
    },
    /// A node refused a write.
    #[error("{step} at endpoint {endpoint} was answered {status}")]
    Refused {
        /// The write, as method and path.
        step: String,
        /// The endpoint.
        endpoint: String,
        /// The node's status; its body is never shown.
        status: StatusCode,
    },
}

/// What the seed wrote and the fixture it leaves.
#[derive(Debug)]
pub struct Seeded {
    /// The fixture.
    pub fixture: Fixture,
    /// Every write the nodes accepted, in order.
    pub written: Vec<Written>,
}

/// The node writes of one run, through `federation`'s node clients.
struct Writer<'f> {
    federation: &'f Federation,
    conveyance: Conveyance,
    withheld: Arc<Withheld>,
    written: Vec<Written>,
}

/// What a node answered one accepted write.
struct Accepted {
    status: StatusCode,
    etag: Option<String>,
}

impl Writer<'_> {
    /// Sends `method path` with `body` of `content_type` to `endpoint`, and
    /// returns the accepted answer; a `409` is returned as accepted only when
    /// `conflict_is_answer`.
    async fn write(
        &self,
        endpoint: &str,
        (method, path): (Method, String),
        (content_type, accept, body): (&'static str, &'static str, Vec<u8>),
        conflict_is_answer: bool,
    ) -> Result<Accepted, SeedError> {
        let step = format!("{method} {path}");
        let client = self.client(endpoint)?;
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(header::ACCEPT, HeaderValue::from_static(accept));
        headers.insert("prefer", HeaderValue::from_static("return=minimal"));
        let request = ClientRequest {
            method,
            path,
            query: None,
            headers,
            body,
        };
        let deadline = Instant::now()
            .checked_add(self.federation.budget().per_node())
            .ok_or(SeedError::Clock)?;
        let options = DispatchOptions::new(deadline, self.conveyance.clone())
            .with_withheld(Arc::clone(&self.withheld))
            .with_request_id(OutboundId::mint());
        let answer =
            client
                .forward(request, &options)
                .await
                .map_err(|source| SeedError::Unanswered {
                    step: step.clone(),
                    endpoint: endpoint.to_owned(),
                    source,
                })?;
        let status = answer.status();
        if status.is_success() || (conflict_is_answer && status == StatusCode::CONFLICT) {
            let etag = answer
                .headers()
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(|value| value.trim_start_matches("W/").trim_matches('"').to_owned());
            return Ok(Accepted { status, etag });
        }
        Err(SeedError::Refused {
            step,
            endpoint: endpoint.to_owned(),
            status,
        })
    }

    /// The node client of `endpoint`.
    fn client(&self, endpoint: &str) -> Result<&NodeClient<NodeTransport>, SeedError> {
        ferrofed_registry::id::EndpointId::new(endpoint)
            .ok()
            .and_then(|id| self.federation.clients().get(&id))
            .ok_or_else(|| SeedError::NoClient(endpoint.to_owned()))
    }

    /// Creates the EHR `ehr_id` at `endpoint` with `status_body`, and returns
    /// whether the node created it (`false` for a `409`: the node holds it).
    async fn create_ehr(
        &mut self,
        endpoint: &str,
        ehr_id: &str,
        body: Vec<u8>,
        what: &str,
    ) -> Result<bool, SeedError> {
        let answer = self
            .write(
                endpoint,
                (Method::PUT, format!("/ehr/{ehr_id}")),
                ("application/json", "application/json", body),
                true,
            )
            .await?;
        if answer.status == StatusCode::CONFLICT {
            return Ok(false);
        }
        self.record(endpoint, format!("EHR {ehr_id} ({what})"));
        Ok(true)
    }

    /// Uploads the vendored template to `endpoint`, unless the node holds it.
    async fn upload_template(&mut self, endpoint: &str, data: &SeedData) -> Result<(), SeedError> {
        let answer = self
            .write(
                endpoint,
                (Method::POST, "/definition/template/adl1.4".to_owned()),
                (
                    "application/xml",
                    "application/xml",
                    data.template().to_vec(),
                ),
                true,
            )
            .await?;
        if answer.status != StatusCode::CONFLICT {
            self.record(endpoint, format!("template {TEMPLATE_ID}"));
        }
        Ok(())
    }

    /// Commits `composition` to `ehr_id` at `endpoint` and returns its
    /// version uid, as the node's `ETag` names it.
    async fn commit(
        &mut self,
        endpoint: &str,
        ehr_id: &str,
        composition: &str,
    ) -> Result<Option<String>, SeedError> {
        let answer = self
            .write(
                endpoint,
                (Method::POST, format!("/ehr/{ehr_id}/composition")),
                (
                    "application/json",
                    "application/json",
                    composition.as_bytes().to_vec(),
                ),
                false,
            )
            .await?;
        let named = answer.etag.clone().unwrap_or_else(|| "no ETag".to_owned());
        self.record(endpoint, format!("composition {named} in EHR {ehr_id}"));
        Ok(answer.etag)
    }

    fn record(&mut self, endpoint: &str, what: String) {
        self.written.push(Written {
            endpoint: endpoint.to_owned(),
            what,
        });
    }
}

/// Seeds `patient` at every member `federation`'s cross-reference names, the
/// spare EHRs beside them, and returns the fixture.
///
/// A member the cross-reference does not name is left unwritten. When a
/// node already holds the `ehr_id` the cross-reference names, or the
/// cross-reference cannot answer, the patient is seeded nowhere and the
/// fixture says why in [`Fixture::shortfall`].
///
/// # Errors
///
/// Returns [`SeedError`] when the gateway cannot convey itself, a node
/// gives no answer to a write, or refuses one.
pub async fn seed(
    federation: &Federation,
    patient: &SyntheticPatient,
    data: &SeedData,
) -> Result<Seeded, SeedError> {
    let snapshot = federation.snapshot();
    let mut members: Vec<Member> = snapshot
        .endpoints()
        .filter_map(|endpoint| {
            let node = snapshot.node(endpoint.node())?;
            Some(Member {
                endpoint: endpoint.id().as_str().to_owned(),
                system_id: node.system_id().as_str().to_owned(),
                organisation: endpoint.managing_organisation().as_str().to_owned(),
                path: endpoint.url().path().trim_end_matches('/').to_owned(),
                active: endpoint.status() == EndpointStatus::Active,
                holding: None,
            })
        })
        .collect();
    let mut writer = Writer {
        federation,
        conveyance: conveyed::gateway(federation).map_err(SeedError::Unconveyed)?,
        withheld: Arc::new(Withheld::new([patient.value().clone()])),
        written: Vec::new(),
    };
    let resolved = match resolve(federation, patient).await? {
        Ok(resolved) => resolved,
        Err(shortfall) => {
            return Ok(Seeded {
                fixture: Fixture {
                    patient: patient.clone(),
                    members,
                    spares: Spares::default(),
                    shortfall: Some(shortfall),
                },
                written: writer.written,
            });
        }
    };

    let status = to_canonical_json(&ehr_status(Some(patient.party_ref())));
    let mut held = Vec::new();
    for (endpoint, ehr_id) in &resolved {
        if !writer
            .create_ehr(
                endpoint,
                ehr_id,
                status.clone().into_bytes(),
                "the synthetic patient",
            )
            .await?
        {
            return Ok(Seeded {
                fixture: Fixture {
                    patient: patient.clone(),
                    members,
                    spares: Spares::default(),
                    shortfall: Some(format!(
                        "endpoint {endpoint} already holds ehr_id {ehr_id}, which the cross-reference names for the synthetic patient and this run did not create, so the run seeds the patient nowhere and reads no EHR it did not write"
                    )),
                },
                written: writer.written,
            });
        }
        held.push((endpoint.clone(), ehr_id.clone()));
    }
    for (index, (endpoint, ehr_id)) in held.iter().enumerate() {
        writer.upload_template(endpoint, data).await?;
        let composition = if index == 0 {
            data.hospital()
        } else {
            data.clinic()
        };
        writer.commit(endpoint, ehr_id, composition).await?;
        if let Some(member) = members
            .iter_mut()
            .find(|member| member.endpoint == *endpoint)
        {
            member.holding = Some(Holding {
                ehr_id: ehr_id.clone(),
                compositions: 1,
            });
        }
    }
    let spares = spares(&mut writer, &held, data).await?;
    Ok(Seeded {
        fixture: Fixture {
            patient: patient.clone(),
            members,
            spares,
            shortfall: None,
        },
        written: writer.written,
    })
}

/// The `(endpoint, ehr_id)` the cross-reference names for `patient` at each
/// member, through the endpoint each member is asked through, or why it
/// names none the run can use.
async fn resolve(
    federation: &Federation,
    patient: &SyntheticPatient,
) -> Result<Result<Vec<(String, String)>, String>, SeedError> {
    let snapshot = federation.snapshot();
    let Some(resolver) = federation.resolver() else {
        return Ok(Err(format!(
            "no cross-reference is configured ({}), so nothing turns the patient into an ehr_id",
            crate::binding::resolver_list()
        )));
    };
    let nodes: Vec<NodeId> = snapshot.nodes().map(|node| node.id().clone()).collect();
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .ok_or(SeedError::Clock)?;
    let answer = resolver
        .resolve(
            &patient.patient_ref()?,
            &nodes,
            &OnBehalfOf::Gateway,
            deadline,
        )
        .await;
    let mut named = Vec::new();
    for node in &nodes {
        match answer.get(node) {
            Some(Resolution::Resolved(ehr_id)) => {
                if let Some(endpoint) = snapshot.asked_through(node) {
                    named.push((endpoint.id().as_str().to_owned(), ehr_id.to_string()));
                }
            }
            Some(Resolution::Unavailable(error)) => {
                return Ok(Err(format!(
                    "the cross-reference did not answer for node {node}: {}",
                    chain(error)
                )));
            }
            Some(Resolution::Unknown) | None => {}
        }
    }
    if named.is_empty() {
        return Ok(Err(
            "the cross-reference resolves the synthetic patient at no member; register it there first"
                .to_owned(),
        ));
    }
    Ok(Ok(named))
}

/// Creates the spare EHRs, each with no subject and a fresh `ehr_id`: one to
/// be probed for at the second holding member, or the first when one holds,
/// one a write to is refused at the first, and one with a composition whose
/// version a versioned write names, at the first.
async fn spares(
    writer: &mut Writer<'_>,
    held: &[(String, String)],
    data: &SeedData,
) -> Result<Spares, SeedError> {
    let Some((first, _)) = held.first() else {
        return Ok(Spares::default());
    };
    let anonymous = to_canonical_json(&ehr_status(None)).into_bytes();
    let fresh = || Uuid::new_v4().to_string();
    let mut created = Vec::new();
    for (endpoint, purpose) in [
        (
            held.get(1).map_or(first, |(second, _)| second),
            "no subject, probed for",
        ),
        (first, "no subject, written to unrouted"),
        (first, "no subject, a versioned write's target"),
    ] {
        let ehr_id = fresh();
        if writer
            .create_ehr(endpoint, &ehr_id, anonymous.clone(), purpose)
            .await?
        {
            created.push(Some((endpoint.clone(), ehr_id)));
        } else {
            created.push(None);
        }
    }
    let mut created = created.into_iter();
    let probe = created.next().flatten();
    let unrouted = created.next().flatten().map(|(_, ehr_id)| ehr_id);
    let versioned = match created.next().flatten() {
        Some((endpoint, ehr_id)) => writer
            .commit(&endpoint, &ehr_id, data.hospital())
            .await?
            .map(|version| (ehr_id, version)),
        None => None,
    };
    Ok(Spares {
        probe,
        unrouted,
        versioned,
    })
}
