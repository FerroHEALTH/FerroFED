// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An access record written as an IHE BALP `AuditEvent` (feature `balp`),
//! through the audit types of `ihe-iti`.
//!
//! The recording system received the request, so the record is the server's
//! side of the `RESTful` pattern its action names (BALP 1.1.4): a query is
//! `IHE.BasicAudit.Query`, a read `IHE.BasicAudit.Read`, a create
//! `IHE.BasicAudit.Create`, an update `IHE.BasicAudit.Update` and a delete
//! `IHE.BasicAudit.Delete`, each its `Patient` variant when the record names
//! one patient, as the implementation guide lists them: the one the request
//! named, or the one the identity service found behind the `ehr_id` it
//! reached. Its `entity:patient` slice holds one patient, so a record that
//! names several writes each and claims the plain pattern. The Delete pattern
//! fixes `outcome` to `0`, so a failed delete claims no profile. The person
//! and the client are the token's (BALP 1.1.4 §3:5.7.5.4), with the provider
//! as an agent of its own, the request id is `entity:transaction`, and the
//! query `entity:query`.
//!
//! The person's agent, `agent:user`, alone carries what Annex II 3.2(b) asks
//! about the natural person: the professional's name as `who.display` and
//! their identifier as an `ihe-otherId` extension typed `NPI`, as BALP
//! 1.1.4 §3:5.7.5.4 maps the IHE IUA `subject_name` and
//! `national_provider_identifier`; the assurance level of the
//! authentication, when one was established, as an `ihe-assuranceLevel`
//! extension coded `low`, `substantial` or `high` (Regulation (EU) No
//! 910/2014 Art 8(2)); and who acts, `person` or `client`, as its `role`.
//!
//! What Annex II 3.2 adds to a BALP record has no element of its own (BALP
//! defines none for a data category), so it rides in entities of type `4`,
//! which no slice of a pattern is discriminated by: one for the categories
//! and the record's retention, one per origin and one per `ehr_id`, each
//! with its `detail` entries named as [`detail`] lists them (no
//! specification governs the names: our own design). The professional and
//! the provider a national contact point relays (Implementing Regulation
//! (EU) 2026/2099 Annex Tables 1 and 2) ride the same way, in one entity
//! `ehds-relayed` that names the contact point and marks them `asserted`,
//! and a correlation identifier the client sent is a `detail` of the request
//! id's entity.
//!
//! The record's security labels ([`crate::label`], Annex II 3.4) are its
//! `meta.security`, which a FHIR repository can limit access by, and each is
//! also the `securityLabel` of the entity it belongs to: the categories' on
//! the `ehds-categories` entity and each origin's on its `origin` entity,
//! which the `RESTful` ATNA mapping keeps as the DICOM
//! `ParticipantObjectSensitivity` (`RESTful` ATNA Rev. 3.6
//! Table 3.20.4.2.2.1-1).
//!
//! An emergency access (Regulation (EU) 2025/327 Art 11(5)) keeps its
//! purposes of use where every purpose is written, in `agent:user`
//! `purposeOfUse` and in `purposeOfEvent`, and is marked in one entity of
//! its own, `ehds-emergency-access`, whose `description` states the mark in words and
//! whose `detail` entries name the purposes that marked it, so a reader of
//! the record need not know which codes the deployment maps, and each member
//! whose consent pre-filter denial was set aside for it.
//!
//! A population access, a query over many patients' data whose record names
//! none of them, is marked in one entity `ehds-population`, whose
//! `description` says in words that the query ran over each origin the
//! record names and that the record confirms no read of any one person's
//! record, and whose `detail` entries name why no `ehr_id` is named and the
//! template and archetype ids the query read (Regulation (EU) 2025/327 Art
//! 9(1)). BALP 1:52.4.1.6 recommends one record per patient "driven by a
//! Policy"; this record names the scope, never a person.

use std::collections::BTreeSet;
use std::sync::Arc;

use ihe_iti::balp::{
    APPLICATION, AuditRecorder, CREATE, CUSTODIAN, DELETE, DESTINATION_ROLE, DOMAIN_RESOURCE,
    Described, Detail, Direction, Entity, EventKind, Exchange, NetworkAddress, OTHER, Peer, READ,
    REQUEST_ID, REST, Reserved, SEARCH, SOURCE_ROLE, SYSTEM_OBJECT, SecurityLabel, UPDATE, What,
};
use ihe_iti::user::{Code, OnBehalfOf, PurposeOfUse, User};
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;

use crate::classify::Classification;
use crate::emergency::Emergency;
use crate::label::Label;
use crate::record::{
    AccessRecord, Action, Origin, Outcome, PatientIdentifier, PatientLookup, Population, Relayed,
};
use crate::sink::{AccessSink, Reservation, SinkError};

/// The names of the `detail` entries a record's added entities carry.
pub mod detail {
    /// A category as `<system>|<code>`, once per category.
    pub const CATEGORY: &str = "ehds-category";
    /// `<system>|<code>:<basis>`, once per basis of each category, the basis
    /// after the last `:`.
    pub const BASIS: &str = "ehds-category-basis";
    /// `<system>|<version>`, once per code system a category is written in
    /// whose version is known.
    pub const CATEGORY_VERSION: &str = "ehds-category-version";
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
    /// Whether the patient behind an `ehr_id` is named: `request-named`,
    /// `found`, `not-found`, `unavailable`, `not-configured` or
    /// `unsupported`.
    pub const PATIENT_LOOKUP: &str = "patient-lookup";
    /// The identifier the client correlates the request by in its own log.
    pub const CORRELATION: &str = "correlation-id";
    /// The national contact point that asserted the relayed professional
    /// and provider, by the issuer of its token.
    pub const CONTACT_POINT: &str = "contact-point";
    /// That the relayed values are the contact point's assertion: `true`.
    pub const ASSERTED: &str = "asserted";
    /// The relayed `country_code`.
    pub const COUNTRY_CODE: &str = "country-code";
    /// The relayed professional's `family_name`.
    pub const FAMILY_NAME: &str = "hp-family-name";
    /// The relayed professional's `given_name`.
    pub const GIVEN_NAME: &str = "hp-given-name";
    /// The relayed professional's `hp_identifier`.
    pub const HP_IDENTIFIER: &str = "hp-identifier";
    /// The agency that issued the relayed `hp_identifier`.
    pub const HP_ISSUING_AUTHORITY: &str = "hp-issuing-authority";
    /// `<system>|<code>`, or `<code>`, once per relayed
    /// `hp_professional_role`.
    pub const HP_ROLE: &str = "hp-professional-role";
    /// The relayed `healthcare_provider_identifier`.
    pub const PROVIDER_IDENTIFIER: &str = "provider-identifier";
    /// The agency that issued the relayed provider identifier.
    pub const PROVIDER_ISSUING_AUTHORITY: &str = "provider-issuing-authority";
    /// The relayed `healthcare_provider_name`.
    pub const PROVIDER_NAME: &str = "provider-name";
    /// The relayed `healthcare_provider_address`.
    pub const PROVIDER_ADDRESS: &str = "provider-address";
    /// The years the record is kept.
    pub const RETENTION_YEARS: &str = "ehds-retention-years";
    /// The first UTC date the record may be deleted on, `YYYY-MM-DD`.
    pub const RETENTION_ENDS: &str = "ehds-retention-ends";
    /// What called for the period: `default`, `unclassified`,
    /// `category:<system>|<code>` or `origin:<endpoint>`.
    pub const RETENTION_GROUND: &str = "ehds-retention-ground";
    /// That the access is an emergency access (Art 11(5)): `true`.
    pub const EMERGENCY_ACCESS: &str = "ehds-emergency-access";
    /// `<system>|<code>`, or `<code>`, once per purpose of use that marked
    /// the access an emergency access.
    pub const EMERGENCY_PURPOSE: &str = "ehds-emergency-purpose";
    /// A member whose consent pre-filter denial was set aside for the
    /// emergency, once per member.
    pub const CONSENT_SET_ASIDE: &str = "ehds-consent-set-aside";
    /// That the access is a population access: `true`.
    pub const POPULATION: &str = "ehds-population";
    /// Why a population access's record names no `ehr_id`: `not-projected`,
    /// `over-limit`, `not-delivered` or `unattributed`.
    pub const POPULATION_REASON: &str = "ehds-population-reason";
    /// How many `ehr_id`s the delivered rows of a population access named.
    pub const POPULATION_EHRS: &str = "ehds-population-ehrs";
    /// The most `ehr_id`s a record names.
    pub const POPULATION_LIMIT: &str = "ehds-population-limit";
}

/// The name of the entity that marks an emergency access.
pub const EMERGENCY_ENTITY: &str = "ehds-emergency-access";

/// The `description` of the entity that marks an emergency access.
pub const EMERGENCY_DESCRIPTION: &str = "Emergency access: the accessor declared that the access \
     was necessary to protect the vital interests of the data subject \
     (Regulation (EU) 2025/327 Art 11(5)).";

/// The name of the entity that marks a population access.
pub const POPULATION_ENTITY: &str = "ehds-population";

/// The `description` of the entity that marks a population access.
pub const POPULATION_DESCRIPTION: &str = "Population access: a query over many \
     patients' data ran over each origin this record names; the record does not name \
     the persons whose records it read, and does not say that it read any one \
     person's record (Regulation (EU) 2025/327 Art 9(1)).";

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

    async fn reserve(&self) -> Result<Box<dyn Reservation>, SinkError> {
        let place = self
            .recorder
            .reserve()
            .await
            .map_err(|error| SinkError(Box::new(error)))?;
        Ok(Box::new(BalpReservation {
            place,
            recorder: Arc::clone(&self.recorder),
            gateway: self.gateway.clone(),
        }))
    }
}

/// The place of one record [`BalpSink`] took: the recorder's own place, or
/// none when the recorder holds no bounded store.
struct BalpReservation {
    place: Option<Box<dyn Reserved>>,
    recorder: Arc<dyn AuditRecorder>,
    gateway: Url,
}

impl std::fmt::Debug for BalpReservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BalpReservation")
            .field("place", &self.place)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Reservation for BalpReservation {
    async fn store(self: Box<Self>, record: AccessRecord) -> Result<(), SinkError> {
        let exchange = exchange(&record, &self.gateway);
        match self.place {
            Some(place) => place.record(exchange).await,
            None => self.recorder.record(exchange).await,
        }
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
    .with_alt_id(accessor.alt_id.clone())
    .with_name(accessor.professional.name.clone())
    .with_provider_identifier(accessor.professional.identifier.clone())
    // NOTE: no specification governs these codes: our own design; BALP 1.1.4 leaves the
    // assuranceLevel vocabulary open, and names no element for who acts.
    .with_assurance(accessor.assurance.map(|level| code(level.code())))
    .with_roles(vec![code(accessor.acting.code())]);
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
        security: security(&record.labels.all()),
    }
}

/// A code no system defines.
fn code(code: &str) -> Code {
    Code {
        system: None,
        code: code.to_owned(),
    }
}

/// What the pattern of `record`'s action fixes.
fn kind(record: &AccessRecord) -> EventKind {
    let patient = patients(record).len() == 1;
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
        details: request
            .correlation
            .iter()
            .map(|correlation| Detail::new(detail::CORRELATION, correlation.clone()))
            .collect(),
        security_labels: Vec::new(),
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
                security_labels: Vec::new(),
            }));
        }
    }
    for patient in patients(record) {
        entities.push(Entity::Patient {
            system: patient.namespace.clone(),
            value: patient.value.clone(),
        });
    }
    for ehr in &record.subject.ehrs {
        entities.push(other(
            Some(ehr.ehr_id.clone()),
            "ehr",
            vec![
                Detail::new(detail::ENDPOINT, ehr.endpoint.clone()),
                Detail::new(detail::PATIENT_LOOKUP, ehr.patient.code()),
            ],
            &[],
        ));
    }
    let mut details = categories(&record.categories);
    if let Some(delivered) = record.delivered {
        details.push(Detail::new(detail::DELIVERED, delivered.to_string()));
    }
    if let Some(name) = &request.stored_query {
        details.push(Detail::new(detail::STORED_QUERY, name.clone()));
    }
    let retention = &record.retention;
    details.extend([
        Detail::new(detail::RETENTION_YEARS, retention.years().get().to_string()),
        Detail::new(detail::RETENTION_ENDS, retention.ends().to_string()),
        Detail::new(detail::RETENTION_GROUND, retention.ground().code()),
    ]);
    entities.push(other(
        None,
        "ehds-categories",
        details,
        record.labels.categories(),
    ));
    for origin in &record.origins {
        entities.push(origin_entity(
            origin,
            record.labels.origin(&origin.endpoint),
        ));
    }
    if let Some(relayed) = &record.accessor.relayed {
        entities.push(other(None, "ehds-relayed", relayed_details(relayed), &[]));
    }
    entities.extend(record.subject.population.as_ref().map(population_entity));
    if let Some(emergency) = &record.emergency {
        entities.push(emergency_entity(emergency));
    }
    entities
}

/// The entity of `origin`, labelled `labels`: how it answered, its node, its
/// `system_id`, its rows and its categories.
fn origin_entity(origin: &Origin, labels: &[Label]) -> Entity {
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
    other(Some(origin.endpoint.clone()), "origin", details, labels)
}

/// The entity that marks an emergency access, naming the purposes that
/// marked it and each member whose consent pre-filter denial was set aside.
fn emergency_entity(emergency: &Emergency) -> Entity {
    let mut details = vec![Detail::new(detail::EMERGENCY_ACCESS, "true")];
    details.extend(emergency.purposes().iter().map(|purpose| {
        Detail::new(
            detail::EMERGENCY_PURPOSE,
            coded(purpose.system.as_deref(), &purpose.code),
        )
    }));
    details.extend(
        emergency
            .consent_set_aside()
            .iter()
            .map(|member| Detail::new(detail::CONSENT_SET_ASIDE, member.clone())),
    );
    Entity::Described(Described {
        what: None,
        kind: OTHER,
        role: None,
        name: Some(EMERGENCY_ENTITY.to_owned()),
        description: Some(EMERGENCY_DESCRIPTION.to_owned()),
        details,
        security_labels: Vec::new(),
    })
}

/// The security labels `labels` are written as.
fn security(labels: &[Label]) -> Vec<SecurityLabel> {
    labels
        .iter()
        .map(|label| SecurityLabel {
            system: label.system().to_owned(),
            version: label.version().map(str::to_owned),
            code: label.code().to_owned(),
        })
        .collect()
}

/// The entity that marks a population access, with why it names no
/// `ehr_id` and the template and archetype ids the query read.
fn population_entity(population: &Population) -> Entity {
    let mut details = vec![
        Detail::new(detail::POPULATION, "true"),
        Detail::new(detail::POPULATION_REASON, population.reason.code()),
    ];
    details.extend(
        population
            .ehrs
            .map(|ehrs| Detail::new(detail::POPULATION_EHRS, ehrs.to_string())),
    );
    details.push(Detail::new(
        detail::POPULATION_LIMIT,
        population.limit.to_string(),
    ));
    for (name, ids) in [
        (detail::TEMPLATE_ID, &population.templates),
        (detail::ARCHETYPE_ID, &population.archetypes),
    ] {
        details.extend(ids.iter().map(|id| Detail::new(name, id.clone())));
    }
    Entity::Described(Described {
        what: None,
        kind: OTHER,
        role: None,
        name: Some(POPULATION_ENTITY.to_owned()),
        description: Some(POPULATION_DESCRIPTION.to_owned()),
        details,
        security_labels: Vec::new(),
    })
}

/// `<system>|<code>`, or `<code>` when no system is named.
fn coded(system: Option<&str>, code: &str) -> String {
    match system {
        Some(system) => format!("{system}|{code}"),
        None => code.to_owned(),
    }
}

/// The `detail` entries of what a national contact point relayed, marked
/// as its assertion.
fn relayed_details(relayed: &Relayed) -> Vec<Detail> {
    let professional = &relayed.professional;
    let provider = &relayed.provider;
    let mut details = vec![
        Detail::new(detail::CONTACT_POINT, relayed.contact_point.clone()),
        Detail::new(detail::ASSERTED, "true"),
        Detail::new(detail::COUNTRY_CODE, relayed.country_code.clone()),
        Detail::new(detail::FAMILY_NAME, professional.family_name.clone()),
        Detail::new(detail::GIVEN_NAME, professional.given_name.clone()),
        Detail::new(detail::HP_IDENTIFIER, professional.identifier.clone()),
        Detail::new(
            detail::HP_ISSUING_AUTHORITY,
            professional.issuing_authority.clone(),
        ),
    ];
    details.extend(
        professional
            .roles
            .iter()
            .map(|role| Detail::new(detail::HP_ROLE, coded(role.system.as_deref(), &role.code))),
    );
    details.extend([
        Detail::new(detail::PROVIDER_IDENTIFIER, provider.identifier.clone()),
        Detail::new(
            detail::PROVIDER_ISSUING_AUTHORITY,
            provider.issuing_authority.clone(),
        ),
        Detail::new(detail::PROVIDER_NAME, provider.name.clone()),
        Detail::new(detail::PROVIDER_ADDRESS, provider.address.clone()),
    ]);
    details
}

/// Every patient `record` names, each once: the one the request named, and
/// each one the identity service found behind an `ehr_id` the access
/// reached.
fn patients(record: &AccessRecord) -> Vec<&PatientIdentifier> {
    let found = record
        .subject
        .ehrs
        .iter()
        .flat_map(|ehr| match &ehr.patient {
            PatientLookup::Found(found) => found.as_slice(),
            _ => &[],
        });
    let mut named: Vec<&PatientIdentifier> = Vec::new();
    for patient in record.subject.patient.iter().chain(found) {
        let seen = named.iter().any(|kept| {
            kept.namespace == patient.namespace
                && kept.value.expose_secret() == patient.value.expose_secret()
        });
        if !seen {
            named.push(patient);
        }
    }
    named
}

/// An entity of type `4` named `name`, identified by `value` when given,
/// labelled `labels`.
fn other(value: Option<String>, name: &str, details: Vec<Detail>, labels: &[Label]) -> Entity {
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
        security_labels: security(labels),
    })
}

/// The `detail` entries of `classified`.
fn categories(classified: &Classification) -> Vec<Detail> {
    let mut details = Vec::new();
    let mut versions = BTreeSet::new();
    for (category, bases) in classified.categories() {
        details.push(Detail::new(detail::CATEGORY, category.token()));
        for basis in bases {
            details.push(Detail::new(
                detail::BASIS,
                format!("{}:{}", category.token(), basis.code()),
            ));
        }
        if let Some(version) = category.version() {
            versions.insert(format!("{}|{version}", category.system()));
        }
    }
    details.extend(
        versions
            .into_iter()
            .map(|version| Detail::new(detail::CATEGORY_VERSION, version)),
    );
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
