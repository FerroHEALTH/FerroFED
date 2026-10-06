// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An access record written as an IHE BALP `AuditEvent` (feature `balp`),
//! through the audit types of `ihe-iti`.
//!
//! The recording system received the request, so the record is the server's
//! side of the `RESTful` pattern its action names (BALP 1.1.4): a query is
//! `IHE.BasicAudit.Query`, a read `IHE.BasicAudit.Read`, a create
//! `IHE.BasicAudit.Create`, an update `IHE.BasicAudit.Update` and a delete
//! `IHE.BasicAudit.Delete`, each its `Patient` variant when the request named
//! the patient, as the implementation guide lists them. The Delete pattern
//! fixes `outcome` to `0`, so a failed delete claims no profile. The person
//! and the client are the token's (BALP 1.1.4 §3:5.7.5.4), with the provider
//! as an agent of its own, the request id is `entity:transaction`, and the
//! query `entity:query`.
//!
//! What Annex II 3.2 adds to a BALP record has no element of its own (BALP
//! defines none for a data category), so it rides in entities of type `4`,
//! which no slice of a pattern is discriminated by: one for the categories,
//! one per origin and one per `ehr_id`, each with its `detail` entries named
//! as [`detail`] lists them (no specification governs the names: our own
//! design).

use std::sync::Arc;

use ihe_iti::balp::{
    APPLICATION, AuditRecorder, CREATE, CUSTODIAN, DELETE, DESTINATION_ROLE, DOMAIN_RESOURCE,
    Described, Detail, Direction, Entity, EventKind, Exchange, NetworkAddress, OTHER, Peer, READ,
    REQUEST_ID, REST, SEARCH, SOURCE_ROLE, SYSTEM_OBJECT, UPDATE, What,
};
use ihe_iti::user::{OnBehalfOf, PurposeOfUse, User};
use secrecy::SecretString;
use url::Url;

use crate::classify::Classification;
use crate::record::{AccessRecord, Action, Outcome};
use crate::sink::{AccessSink, SinkError};

/// The names of the `detail` entries a record's added entities carry.
pub mod detail {
    /// A category code, once per category.
    pub const CATEGORY: &str = "ehds-category";
    /// `<category>:<basis>`, once per basis of each category.
    pub const BASIS: &str = "ehds-category-basis";
    /// `true` when the access reached data of no category.
    pub const NO_CATEGORY: &str = "ehds-no-category";
    /// Why the access is unclassified.
    pub const UNCLASSIFIED: &str = "ehds-unclassified";
    /// A template id of the evidence.
    pub const TEMPLATE_ID: &str = "template-id";
    /// An archetype id of the evidence.
    pub const ARCHETYPE_ID: &str = "archetype-id";
    /// A version uid of a root object the access reached.
    pub const VERSION_UID: &str = "version-uid";
    /// An id the category map holds no key for.
    pub const UNMAPPED_ID: &str = "unmapped-id";
    /// The digest of the category map.
    pub const MAP_DIGEST: &str = "category-map-digest";
    /// The rows or objects delivered.
    pub const DELIVERED: &str = "delivered";
    /// The stored query executed.
    pub const STORED_QUERY: &str = "stored-query";
    /// The node behind an origin.
    pub const NODE: &str = "node";
    /// The `system_id` of the node behind an origin.
    pub const SYSTEM_ID: &str = "system-id";
    /// How an origin answered.
    pub const STATUS: &str = "status";
    /// The rows an origin contributed.
    pub const ROWS: &str = "rows";
    /// The endpoint an `ehr_id` is held at.
    pub const ENDPOINT: &str = "endpoint";
}

/// The sink that writes each record as a BALP `AuditEvent` and hands it to
/// an `ihe-iti` audit recorder, such as one that spools it for an ATNA
/// Audit Record Repository.
pub struct BalpSink {
    recorder: Arc<dyn AuditRecorder>,
    gateway: Url,
}

impl std::fmt::Debug for BalpSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BalpSink").finish_non_exhaustive()
    }
}

impl BalpSink {
    /// The sink that records through `recorder`, naming the recording system
    /// by `gateway`, the base its requests arrive at.
    #[must_use]
    pub fn new(recorder: Arc<dyn AuditRecorder>, gateway: Url) -> Self {
        Self { recorder, gateway }
    }
}

#[async_trait::async_trait]
impl AccessSink for BalpSink {
    async fn store(&self, record: AccessRecord) -> Result<(), SinkError> {
        self.recorder
            .record(exchange(&record, &self.gateway))
            .await
            .map_err(|error| SinkError(Box::new(error)))
    }
}

/// The BALP exchange `record` is written as, the recording system named by
/// `gateway`.
#[must_use]
pub fn exchange(record: &AccessRecord, gateway: &Url) -> Exchange {
    let accessor = &record.accessor;
    let user = User::new(
        accessor.issuer.clone(),
        accessor.subject.clone(),
        accessor.client_id.clone(),
    )
    .with_audience(accessor.audience.clone())
    .with_purposes(
        accessor
            .purposes
            .iter()
            .map(|purpose| PurposeOfUse {
                system: purpose.system.clone(),
                code: purpose.code.clone(),
            })
            .collect(),
    )
    .with_organisation(accessor.provider.clone())
    .with_alt_id(accessor.professional.clone());
    let client = Peer {
        who: accessor.client_id.clone(),
        network: record.request.client_address.map_or_else(
            || NetworkAddress::MachineName(String::from("unknown")),
            NetworkAddress::Ip,
        ),
    };
    Exchange {
        kind: kind(record),
        recorded: record.recorded,
        outcome: match record.outcome {
            Outcome::Success => ihe_iti::balp::Outcome::Success,
            Outcome::MinorFailure => ihe_iti::balp::Outcome::MinorFailure,
            Outcome::SeriousFailure => ihe_iti::balp::Outcome::SeriousFailure,
        },
        direction: Direction::Received {
            client,
            endpoint: gateway.clone(),
        },
        on_behalf: OnBehalfOf::User(user),
        entities: entities(record),
    }
}

/// What the pattern of `record`'s action fixes.
fn kind(record: &AccessRecord) -> EventKind {
    let patient = record.subject.patient.is_some();
    let profile = |plain: &'static str, with_patient: &'static str| {
        if patient { with_patient } else { plain }
    };
    match record.action {
        Action::Query => EventKind {
            profile: profile(
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Query",
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientQuery",
            ),
            event_type: REST,
            subtypes: &[SEARCH],
            action: "E",
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        Action::Read => EventKind {
            profile: profile(
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Read",
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientRead",
            ),
            event_type: REST,
            subtypes: &[READ],
            action: "R",
            client: DESTINATION_ROLE,
            server: SOURCE_ROLE,
        },
        Action::Create => EventKind {
            profile: profile(
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Create",
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientCreate",
            ),
            event_type: REST,
            subtypes: &[CREATE],
            action: "C",
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        Action::Update => EventKind {
            profile: profile(
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Update",
                "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientUpdate",
            ),
            event_type: REST,
            subtypes: &[UPDATE],
            action: "U",
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        Action::Delete => EventKind {
            profile: if record.outcome == Outcome::Success {
                profile(
                    "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Delete",
                    "https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientDelete",
                )
            } else {
                ""
            },
            event_type: REST,
            subtypes: &[DELETE],
            action: "D",
            client: APPLICATION,
            server: CUSTODIAN,
        },
    }
}

/// The entities of `record`: the request id, the query or the resource,
/// the patient, each `ehr_id`, the categories, and each origin.
fn entities(record: &AccessRecord) -> Vec<Entity> {
    let request = &record.request;
    let mut entities = vec![Entity::Described(Described {
        what: Some(What::Identifier {
            system: None,
            value: request.id.clone(),
        }),
        kind: REQUEST_ID,
        role: None,
        name: None,
        description: None,
        details: Vec::new(),
    })];
    match record.action {
        Action::Query => entities.push(Entity::Query(
            request
                .query
                .clone()
                .unwrap_or_else(|| SecretString::from(request.operation.clone())),
        )),
        Action::Read | Action::Create | Action::Update | Action::Delete => {
            entities.push(Entity::Described(Described {
                what: Some(What::Reference {
                    reference: request
                        .resource
                        .clone()
                        .unwrap_or_else(|| request.operation.clone()),
                    resource_type: None,
                }),
                kind: SYSTEM_OBJECT,
                role: Some(DOMAIN_RESOURCE),
                name: Some(request.operation.clone()),
                description: None,
                details: Vec::new(),
            }));
        }
    }
    if let Some(patient) = &record.subject.patient {
        entities.push(Entity::Patient {
            system: patient.namespace.clone(),
            value: patient.value.clone(),
        });
    }
    for ehr in &record.subject.ehrs {
        entities.push(other(
            Some(ehr.ehr_id.clone()),
            "ehr",
            vec![Detail::new(detail::ENDPOINT, ehr.endpoint.clone())],
        ));
    }
    let mut details = categories(&record.categories);
    if let Some(delivered) = record.delivered {
        details.push(Detail::new(detail::DELIVERED, delivered.to_string()));
    }
    if let Some(name) = &request.stored_query {
        details.push(Detail::new(detail::STORED_QUERY, name.clone()));
    }
    entities.push(other(None, "ehds-categories", details));
    for origin in &record.origins {
        let mut details = vec![Detail::new(detail::STATUS, origin.status.clone())];
        details.extend(
            origin
                .node
                .iter()
                .map(|node| Detail::new(detail::NODE, node.clone())),
        );
        details.extend(
            origin
                .system_id
                .iter()
                .map(|system_id| Detail::new(detail::SYSTEM_ID, system_id.clone())),
        );
        details.extend(
            origin
                .rows
                .map(|rows| Detail::new(detail::ROWS, rows.to_string())),
        );
        details.extend(origin.categories.iter().flat_map(categories));
        entities.push(other(Some(origin.endpoint.clone()), "origin", details));
    }
    entities
}

/// An entity of type `4` named `name`, identified by `value` when given.
fn other(value: Option<String>, name: &str, details: Vec<Detail>) -> Entity {
    Entity::Described(Described {
        what: value.map(|value| What::Identifier {
            system: None,
            value,
        }),
        kind: OTHER,
        role: None,
        name: Some(name.to_owned()),
        description: None,
        details,
    })
}

/// The `detail` entries of `classified`.
fn categories(classified: &Classification) -> Vec<Detail> {
    let mut details = Vec::new();
    for (category, bases) in classified.categories() {
        details.push(Detail::new(detail::CATEGORY, category.code()));
        for basis in bases {
            details.push(Detail::new(
                detail::BASIS,
                format!("{}:{}", category.code(), basis.code()),
            ));
        }
    }
    if classified.is_no_category() {
        details.push(Detail::new(detail::NO_CATEGORY, "true"));
    }
    if let Some(unclassified) = classified.unclassified() {
        details.push(Detail::new(detail::UNCLASSIFIED, unclassified.code()));
    }
    for (name, ids) in [
        (detail::TEMPLATE_ID, classified.templates()),
        (detail::ARCHETYPE_ID, classified.archetypes()),
        (detail::VERSION_UID, classified.versions()),
        (detail::UNMAPPED_ID, classified.unmapped()),
    ] {
        details.extend(ids.iter().map(|id| Detail::new(name, id.clone())));
    }
    if let Some(digest) = classified.digest() {
        details.push(Detail::new(detail::MAP_DIGEST, digest));
    }
    details
}
