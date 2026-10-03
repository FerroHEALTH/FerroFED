// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The provenance headers of an answer, `openEHR-federation-endpoint` and
//! `openEHR-federation-system-id` (§7a.3, §9.6, N31).
//!
//! The answer to a request routed or dispatched to a single node names that
//! node's endpoint and its `system_id`, whatever the node answered (N31). A
//! federated AQL answer lists the endpoints that contributed rows to it, and
//! their nodes' `system_id`s, in registry order (§7a.3);
//! `meta.federation.endpoints[]` stays the normative carrier there and names
//! every endpoint, listed or not (§11.1). Both headers carry registry
//! endpoint ids and openEHR `system_id`s alone, never a value the request
//! carried (§5.4.1, N33).

use std::collections::BTreeSet;

use axum::response::Response;
use ferrofed_engine::fanout::Plan;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use http::{HeaderName, HeaderValue, StatusCode};
use openehr_federation::headers;
use openehr_federation::meta::FederationMeta;
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;

use crate::facade::scoped::Owner;

// NOTE: §7a.3, §8.4: no specification fixes the list form; the request header's
// form is reused, as its own example spells it.
const SEPARATOR: &str = ", ";

/// The endpoints an answer names as having acted for it, each with its
/// node's `system_id` (§7a.3, N31).
#[derive(Debug, Clone)]
pub(crate) struct Provenance {
    acting: Vec<Acting>,
}

/// One endpoint an answer names, and its node's `system_id` when the
/// registry knows it.
#[derive(Debug, Clone)]
struct Acting {
    endpoint: String,
    system_id: Option<String>,
}

impl Provenance {
    /// The provenance of an answer `endpoint` of `snapshot` acted for alone.
    pub(crate) fn of(snapshot: &RegistrySnapshot, endpoint: &Endpoint) -> Self {
        let acting = Acting {
            endpoint: endpoint.id().as_str().to_owned(),
            system_id: snapshot
                .node(endpoint.node())
                .map(|node| node.system_id().as_str().to_owned()),
        };
        Self {
            acting: vec![acting],
        }
    }

    /// The provenance listing the endpoints of `federation`, the
    /// `meta.federation` record of an answer, that `named` picks.
    fn listed(federation: &FederationMeta, named: impl Fn(&EndpointOutcome) -> bool) -> Self {
        let acting = federation
            .endpoints()
            .iter()
            .filter(|record| named(record))
            .map(|record| Acting {
                endpoint: record.id().as_str().to_owned(),
                system_id: record.system_id().map(str::to_owned),
            })
            .collect();
        Self { acting }
    }

    /// `response` with `openEHR-federation-endpoint` set to the acting
    /// endpoints and `openEHR-federation-system-id` to their nodes'
    /// `system_id`s, position for position, each a list in the form of the
    /// request header (§8.4).
    ///
    /// An answer no endpoint acted for carries neither header. The second is
    /// left out when the registry knows no `system_id` for an acting
    /// endpoint, so a position in one list always names the same node as in
    /// the other.
    #[expect(
        clippy::expect_used,
        reason = "registry ids are ASCII letters, digits and . - _, and a system_id is an openEHR UID, so a list of either is a valid header value"
    )]
    pub(crate) fn stamp(self, mut response: Response) -> Response {
        if self.acting.is_empty() {
            return response;
        }
        let endpoints: Vec<&str> = self
            .acting
            .iter()
            .map(|acting| acting.endpoint.as_str())
            .collect();
        let system_ids: Option<Vec<&str>> = self
            .acting
            .iter()
            .map(|acting| acting.system_id.as_deref())
            .collect();
        let fields = response.headers_mut();
        let endpoint = HeaderValue::try_from(endpoints.join(SEPARATOR))
            .expect("a list of registry endpoint ids should be a valid header value");
        fields.insert(header_name(headers::ENDPOINT), endpoint);
        if let Some(system_ids) = system_ids {
            let system_id = HeaderValue::try_from(system_ids.join(SEPARATOR))
                .expect("a list of registry system_ids should be a valid header value");
            fields.insert(header_name(headers::SYSTEM_ID), system_id);
        }
        response
    }
}

/// Where a federated AQL query went, which decides the endpoints its answer
/// names (§7a.3, N31).
#[derive(Debug, Clone)]
pub(crate) enum Dispatch<'a> {
    /// A query scoped to one `ehr_id`, routed to the one member that owns it
    /// (N29, §12.5.1).
    Routed(&'a Endpoint),
    /// A query directed at exactly one endpoint, and dispatched to it.
    Single(EndpointId),
    /// Every other query: a fan-out, or a directed query that asked no node.
    Federated,
}

impl<'a> Dispatch<'a> {
    /// Where a query goes that was `routed` by its `ehr_id`, or directed at
    /// the endpoints `named`, and is dispatched under `plan`.
    ///
    /// A query directed at one endpoint that `plan` asks nothing, because
    /// the patient is `not-resolved` there or resolution failed, went to no
    /// node, so it is [`Dispatch::Federated`] and lists who contributed rows.
    pub(crate) fn of(
        routed: Option<Owner<'a>>,
        named: Option<&BTreeSet<EndpointId>>,
        plan: &Plan,
    ) -> Self {
        if let Some(owner) = routed {
            return Self::Routed(owner.endpoint);
        }
        named
            .filter(|named| named.len() == 1)
            .and_then(BTreeSet::first)
            .and_then(|endpoint| plan.dispatched().find(|dispatched| *dispatched == endpoint))
            .map_or(Self::Federated, |endpoint| Self::Single(endpoint.clone()))
    }

    /// The provenance of the answer, with the status `status` and the
    /// `meta.federation` record `federation`, to a query dispatched this
    /// way over `snapshot`.
    ///
    /// A query that went to a single node names it, whatever that node
    /// answered (N31). Every other answer lists the endpoints that
    /// contributed rows: `active`, with a `row_count` above 0, counted before
    /// federation-level `DISTINCT`, dedup and `LIMIT` as §9.5 counts what an
    /// endpoint contributed. A failing answer returns no rows, so it lists
    /// none (§11.4).
    pub(crate) fn provenance(
        self,
        snapshot: &RegistrySnapshot,
        federation: &FederationMeta,
        status: StatusCode,
    ) -> Provenance {
        match self {
            Self::Routed(endpoint) => Provenance::of(snapshot, endpoint),
            Self::Single(endpoint) => Provenance::listed(federation, |record| {
                record.id().as_str() == endpoint.as_str()
            }),
            Self::Federated => Provenance::listed(federation, |record| {
                status == StatusCode::OK && contributed(record)
            }),
        }
    }
}

/// Whether `record` contributed rows to the answer (§9.5, §11.1).
fn contributed(record: &EndpointOutcome) -> bool {
    record.status() == EndpointStatus::Active && record.row_count().is_some_and(|rows| rows > 0)
}

/// The field name `name` spells, lower-cased as HTTP/2 sends it.
#[expect(
    clippy::expect_used,
    reason = "the federation's header names are ASCII tokens, which are valid field names"
)]
fn header_name(name: &str) -> HeaderName {
    HeaderName::from_bytes(name.as_bytes())
        .expect("a federation header name should be a valid field name")
}
