// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! ITI-81 Retrieve ATNA Audit Event, as the harness repository answers it
//! (IHE `RESTful` ATNA Rev. 3.6 §3.81).
//!
//! `GET [base]/AuditEvent?date=ge<start>&date=le<stop>&<query>` searches the
//! records the repository accepted (§3.81.4.1.2):
//!
//! - `date`, at least one, a `ge`, `gt`, `le`, `lt` or `eq` prefix on a date
//!   or an instant, matched against `recorded` (§3.81.4.1.2.1); a date is the
//!   UTC day it names;
//! - `patient.identifier`, a token matched against the identifier of an
//!   entity whose role is Patient (`1`);
//! - `entity.identifier`, against the identifier of any entity;
//! - `agent.identifier`, against the identifier of any agent's `who`;
//! - `outcome`, against `outcome`.
//!
//! A token is `<system>|<value>`, `|<value>` for an identifier with no
//! system, or `<value>` for any system; `,` separates alternatives within
//! one parameter, and parameters combine with AND (§3.81.4.1.2.2). Every
//! other parameter is ignored (§3.81.4.1.3). A search with no `date` is
//! `400`, which §3.81.4.2.2 allows; one that matches nothing is `200` with
//! an empty `Bundle`. The answer is a `searchset` `Bundle` of the matching
//! records (§3.81.4.2.2.2). Each search is itself recorded, locally, as the
//! `Audit Log Used` event of §3.81.5.1.
//!
//! **This is a test device, not an Audit Record Repository**: it holds the
//! wire of the transaction to the vendored supplement and nothing more (no
//! specification governs the harness: our own design). It matches
//! `patient.identifier` against entities by their Patient role, since it
//! resolves no reference, and against no agent.

use std::sync::{Arc, PoisonError};

use base64::Engine as _;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::r4::audit_event::{
    AuditEvent, AuditEventAgent, AuditEventEntity, AuditEventSource,
};
use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use jiff::Timestamp;
use jiff::civil::Date;
use wiremock::{Request, Respond, ResponseTemplate};

use super::Held;

/// The DICOM code system, which names the `Audit Log Used` event.
const DCM: &str = "http://dicom.nema.org/resources/ontology/DCM";

/// The media type of every answer.
const FHIR_JSON: &str = "application/fhir+json";

/// The answer to an ITI-81 search over what `Held` keeps.
pub(super) struct Search {
    /// What the repository holds.
    pub(super) held: Arc<Held>,
    /// The repository's FHIR base, which names its records and its log.
    pub(super) base: String,
}

impl Respond for Search {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let pairs: Vec<(String, String)> = request
            .url
            .query_pairs()
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let query = match Query::read(&pairs) {
            Ok(query) => query,
            Err(why) => return refused(why),
        };
        let records = self
            .held
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let entry: Vec<BundleEntry> = records
            .iter()
            .enumerate()
            .filter_map(|(index, text)| decode(text).map(|event| (index, event)))
            .filter(|(_, event)| query.matches(event))
            .map(|(index, event)| BundleEntry {
                full_url: Some(format!("{}AuditEvent/{index}", self.base).into()),
                resource: Some(Resource::AuditEvent(Box::new(event))),
                ..BundleEntry::default()
            })
            .collect();
        let total = u32::try_from(entry.len()).unwrap_or(u32::MAX);
        let used = log_used(&self.base, request.url.query().unwrap_or_default());
        if let Ok(text) = serde_json::to_string(&used) {
            self.held
                .used
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(text);
        }
        let bundle = Bundle {
            r#type: "searchset".into(),
            total: Some(total.into()),
            entry,
            ..Bundle::default()
        };
        answer(200, &bundle)
    }
}

/// One alternative of a token parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    /// `None` for any system, `Some("")` for an identifier with none.
    system: Option<String>,
    value: String,
}

impl Token {
    /// The alternatives `text` names.
    fn alternatives(text: &str) -> Vec<Self> {
        text.split(',')
            .map(|alternative| match alternative.split_once('|') {
                Some((system, value)) => Self {
                    system: Some(system.to_owned()),
                    value: value.to_owned(),
                },
                None => Self {
                    system: None,
                    value: alternative.to_owned(),
                },
            })
            .collect()
    }

    /// Whether `identifier` matches this alternative.
    fn matches(&self, identifier: &Identifier) -> bool {
        let value = identifier
            .value
            .as_ref()
            .and_then(|value| value.value.as_deref());
        let system = identifier
            .system
            .as_ref()
            .and_then(|system| system.value.as_deref());
        value == Some(self.value.as_str())
            && match self.system.as_deref() {
                None => true,
                Some("") => system.is_none(),
                Some(asked) => system == Some(asked),
            }
    }
}

/// A `date` prefix (FHIR R4 Search §3.1.1.4.10).
#[derive(Debug, Clone, Copy)]
enum Prefix {
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
}

/// One `date` bound: the prefix and the span `[low, high)` its value names.
#[derive(Debug, Clone, Copy)]
struct DateBound {
    prefix: Prefix,
    low: Timestamp,
    high: Timestamp,
}

impl DateBound {
    /// The bound `text` writes, or why it is refused.
    fn read(text: &str) -> Result<Self, &'static str> {
        let (prefix, value) = [
            ("ge", Prefix::Ge),
            ("gt", Prefix::Gt),
            ("le", Prefix::Le),
            ("lt", Prefix::Lt),
            ("eq", Prefix::Eq),
        ]
        .into_iter()
        .find_map(|(written, prefix)| text.strip_prefix(written).map(|rest| (prefix, rest)))
        .unwrap_or((Prefix::Eq, text));
        if let Ok(instant) = value.parse::<Timestamp>() {
            return Ok(Self {
                prefix,
                low: instant,
                high: instant,
            });
        }
        let day: Date = value
            .parse()
            .map_err(|_unparsed| "a date parameter is neither a date nor an instant")?;
        let start = |day: Date| {
            day.to_datetime(jiff::civil::Time::midnight())
                .to_zoned(jiff::tz::TimeZone::UTC)
                .map(|zoned| zoned.timestamp())
        };
        let low = start(day).map_err(|_unrepresented| "a date parameter is out of range")?;
        let high = day
            .tomorrow()
            .and_then(start)
            .map_err(|_unrepresented| "a date parameter is out of range")?;
        Ok(Self { prefix, low, high })
    }

    /// Whether `recorded` falls within this bound.
    fn admits(&self, recorded: Timestamp) -> bool {
        let exact = self.low == self.high;
        match self.prefix {
            Prefix::Ge => recorded >= self.low,
            Prefix::Gt => {
                if exact {
                    recorded > self.high
                } else {
                    recorded >= self.high
                }
            }
            Prefix::Le => {
                if exact {
                    recorded <= self.high
                } else {
                    recorded < self.high
                }
            }
            Prefix::Lt => recorded < self.low,
            Prefix::Eq => {
                if exact {
                    recorded == self.low
                } else {
                    recorded >= self.low && recorded < self.high
                }
            }
        }
    }
}

/// One ITI-81 search, as its query string writes it.
#[derive(Debug, Default)]
struct Query {
    dates: Vec<DateBound>,
    patients: Vec<Vec<Token>>,
    entities: Vec<Vec<Token>>,
    agents: Vec<Vec<Token>>,
    outcomes: Vec<Vec<String>>,
}

impl Query {
    /// The search `pairs` write, or why it is refused.
    fn read(pairs: &[(String, String)]) -> Result<Self, &'static str> {
        let mut query = Self::default();
        for (name, value) in pairs {
            match name.as_str() {
                "date" => query.dates.push(DateBound::read(value)?),
                "patient.identifier" => query.patients.push(Token::alternatives(value)),
                "entity.identifier" => query.entities.push(Token::alternatives(value)),
                "agent.identifier" => query.agents.push(Token::alternatives(value)),
                "outcome" => query.outcomes.push(
                    Token::alternatives(value)
                        .into_iter()
                        .map(|token| token.value)
                        .collect(),
                ),
                _ => {}
            }
        }
        if query.dates.is_empty() {
            return Err("at least one date parameter is required (ITI-81 §3.81.4.1.2.1)");
        }
        Ok(query)
    }

    /// Whether `event` matches every parameter of this search.
    fn matches(&self, event: &AuditEvent) -> bool {
        let recorded = event
            .recorded
            .value
            .as_deref()
            .and_then(|text| text.parse::<Timestamp>().ok());
        let Some(recorded) = recorded else {
            return false;
        };
        let patient_entities: Vec<&Identifier> = event
            .entity
            .iter()
            .filter(|entity| is_patient(entity))
            .filter_map(identifier_of)
            .collect();
        let entities: Vec<&Identifier> = event.entity.iter().filter_map(identifier_of).collect();
        let agents: Vec<&Identifier> = event.agent.iter().filter_map(who_of).collect();
        let outcome = event
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.value.as_deref());
        self.dates.iter().all(|bound| bound.admits(recorded))
            && self
                .patients
                .iter()
                .all(|any| any_of(any, &patient_entities))
            && self.entities.iter().all(|any| any_of(any, &entities))
            && self.agents.iter().all(|any| any_of(any, &agents))
            && self
                .outcomes
                .iter()
                .all(|any| any.iter().any(|code| outcome == Some(code.as_str())))
    }
}

/// Whether one of `alternatives` matches one of `identifiers`.
fn any_of(alternatives: &[Token], identifiers: &[&Identifier]) -> bool {
    alternatives.iter().any(|token| {
        identifiers
            .iter()
            .any(|identifier| token.matches(identifier))
    })
}

/// Whether `entity` plays the Patient role (`1`, object-role).
fn is_patient(entity: &AuditEventEntity) -> bool {
    entity
        .role
        .as_ref()
        .and_then(|role| role.code.as_ref())
        .and_then(|code| code.value.as_deref())
        == Some("1")
}

/// The identifier `entity.what` carries.
fn identifier_of(entity: &AuditEventEntity) -> Option<&Identifier> {
    entity
        .what
        .as_ref()
        .and_then(|what| what.identifier.as_deref())
}

/// The identifier `agent.who` carries.
fn who_of(agent: &AuditEventAgent) -> Option<&Identifier> {
    agent.who.as_ref().and_then(|who| who.identifier.as_deref())
}

/// The record `text` is, when it decodes as an R4 `AuditEvent`.
fn decode(text: &str) -> Option<AuditEvent> {
    // NOTE: no specification governs this: our own design; the harness keeps every body
    // it accepted, and one that is no AuditEvent is no record a search can match.
    let value: Value = serde_json::from_str(text).ok()?;
    let root = Path::root("AuditEvent");
    let object = expect_object(&value, &root).ok()?;
    AuditEvent::from_json(object, &mut Path::root("AuditEvent")).ok()
}

/// A coding of `system` and `code`, displayed as `display`.
fn coding(system: &str, code: &str, display: &str) -> Coding {
    Coding {
        system: Some(system.into()),
        code: Some(code.into()),
        display: Some(display.into()),
        ..Coding::default()
    }
}

/// The `Audit Log Used` event of a search of the log at `base` with `query`
/// (§3.81.5.1): `EV(110101, DCM, "Audit Log Used")`, action `R`, event type
/// `ITI-81`, the Audit Consumer as the source, the repository as the
/// destination, and the log as a security resource, the query base64.
fn log_used(base: &str, query: &str) -> AuditEvent {
    let role = |code: &str, display: &str| CodeableConcept {
        coding: vec![coding(DCM, code, display)],
        ..CodeableConcept::default()
    };
    AuditEvent {
        r#type: coding(DCM, "110101", "Audit Log Used"),
        subtype: vec![coding(
            "urn:ihe:event-type-code",
            "ITI-81",
            "Retrieve ATNA Audit Event",
        )],
        action: Some("R".into()),
        recorded: Timestamp::now().to_string().into(),
        outcome: Some("0".into()),
        agent: vec![
            AuditEventAgent {
                r#type: Some(role("110153", "Source Role ID")),
                who: Some(Reference {
                    display: Some("the Audit Consumer".into()),
                    ..Reference::default()
                }),
                requestor: true.into(),
                ..AuditEventAgent::default()
            },
            AuditEventAgent {
                r#type: Some(role("110152", "Destination Role ID")),
                who: Some(Reference {
                    identifier: Some(Box::new(Identifier {
                        value: Some(format!("{base}AuditEvent").into()),
                        ..Identifier::default()
                    })),
                    ..Reference::default()
                }),
                requestor: false.into(),
                ..AuditEventAgent::default()
            },
        ],
        source: AuditEventSource {
            observer: Reference {
                display: Some("the harness Audit Record Repository".into()),
                ..Reference::default()
            },
            ..AuditEventSource::default()
        },
        entity: vec![AuditEventEntity {
            what: Some(Reference {
                identifier: Some(Box::new(Identifier {
                    value: Some(format!("{base}AuditEvent").into()),
                    ..Identifier::default()
                })),
                ..Reference::default()
            }),
            r#type: Some(coding(
                "http://terminology.hl7.org/CodeSystem/audit-entity-type",
                "2",
                "System Object",
            )),
            role: Some(coding(
                "http://terminology.hl7.org/CodeSystem/object-role",
                "13",
                "Security Resource",
            )),
            name: Some("Security Audit Log".into()),
            query: Some(
                base64::engine::general_purpose::STANDARD
                    .encode(query.as_bytes())
                    .into(),
            ),
            ..AuditEventEntity::default()
        }],
        ..AuditEvent::default()
    }
}

/// `body` as FHIR JSON under `status`.
fn answer(status: u16, body: &impl serde::Serialize) -> ResponseTemplate {
    match serde_json::to_vec(body) {
        Ok(bytes) => ResponseTemplate::new(status).set_body_raw(bytes, FHIR_JSON),
        Err(_unwritten) => ResponseTemplate::new(500),
    }
}

/// The `400` of a search the transaction refuses, for `why`.
fn refused(why: &str) -> ResponseTemplate {
    answer(
        400,
        &OperationOutcome {
            issue: vec![OperationOutcomeIssue {
                severity: "error".into(),
                code: "invalid".into(),
                details: Some(CodeableConcept {
                    text: Some(why.into()),
                    ..CodeableConcept::default()
                }),
                ..OperationOutcomeIssue::default()
            }],
            ..OperationOutcome::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use fhir_types::r4::identifier::Identifier;
    use jiff::Timestamp;

    use super::{DateBound, Query, Token};

    fn identifier(system: Option<&str>, value: &str) -> Identifier {
        Identifier {
            system: system.map(Into::into),
            value: Some(value.into()),
            ..Identifier::default()
        }
    }

    fn at(text: &str) -> Timestamp {
        text.parse().expect("an instant")
    }

    fn one(text: &str) -> Token {
        let mut alternatives = Token::alternatives(text);
        assert_eq!(1, alternatives.len(), "one alternative in {text}");
        alternatives.remove(0)
    }

    /// FHIR R4 Search: `|value` names an identifier with no system, a bare
    /// value any system, and `system|value` that system alone.
    #[test]
    fn a_token_matches_by_its_system_form() {
        let none = identifier(None, "v");
        let some = identifier(Some("urn:oid:2.999.1"), "v");
        let (bare, empty, named) = (one("v"), one("|v"), one("urn:oid:2.999.1|v"));
        assert!(bare.matches(&none) && bare.matches(&some), "any system");
        assert!(empty.matches(&none) && !empty.matches(&some), "no system");
        assert!(!named.matches(&none) && named.matches(&some), "that system");
        assert_eq!(
            3,
            Token::alternatives("a,b,c").len(),
            "`,` separates alternatives"
        );
    }

    /// §3.81.4.1.2.1: a date bound is the UTC day it names, inclusive at
    /// both ends for `ge` and `le`.
    #[test]
    fn a_date_bound_covers_its_whole_day() {
        let ge = DateBound::read("ge2027-03-05").expect("a bound");
        let le = DateBound::read("le2027-03-05").expect("a bound");
        assert!(ge.admits(at("2027-03-05T00:00:00Z")), "ge, the day's start");
        assert!(!ge.admits(at("2027-03-04T23:59:59Z")), "ge, the day before");
        assert!(le.admits(at("2027-03-05T23:59:59Z")), "le, the day's end");
        assert!(!le.admits(at("2027-03-06T00:00:00Z")), "le, the day after");
        let eq = DateBound::read("2027-03-05").expect("a bound");
        assert!(eq.admits(at("2027-03-05T12:00:00Z")), "no prefix is eq");
        assert!(DateBound::read("ge-not-a-date").is_err(), "no date");
    }

    #[test]
    fn a_search_with_no_date_is_refused() {
        let pairs = vec![("patient.identifier".to_owned(), "a|b".to_owned())];
        assert!(Query::read(&pairs).is_err(), "no date");
        let pairs = vec![("date".to_owned(), "ge2027-01-01".to_owned())];
        assert!(Query::read(&pairs).is_ok(), "a date");
    }
}
