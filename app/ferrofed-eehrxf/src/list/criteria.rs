// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU national contact point API `DocumentReference` search
//! parameters the
//! gateway evaluates on every entry it lists (`myhealth.eu.fhir.ncp-api`
//! 9.1.1-ci, `CapabilityStatementMyHealthEu`).
//!
//! Each value is read with the FHIR R4 search grammar of `fhir-search`: a
//! token for `category`, `contenttype`, `status`, `modality` and
//! `bodysite`, and a date with its prefix for `creation`, `date` and
//! `period` (FHIR R4 search §3.1.1.4.5, §3.1.1.4.10). Repeated parameters
//! must all match and the comma-separated values of one are alternatives
//! (§3.1.1.4.2). An
//! entry that lacks the element a parameter reads matches none of its
//! values.

use fhir_search::date::{DateError, DateValue, Instants, SearchDate};
use fhir_search::escape::split;
use fhir_search::prefix::Prefix;
use fhir_search::token::{Token, TokenError};
use fhir_types::codec::Value;
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The R5 cross-version extension the imaging modality is carried in
/// (`SearchParameter-DocumentReference-modality`).
const MODALITY: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.modality";

/// The R5 cross-version extension the body site is carried in, as
/// `DocumentReference-minimal-myhealtheu` names it.
// NOTE: NCPeH API 9.1.1-ci SearchParameter-DocumentReference-bodysite spells the URL with a
// lower-case `bodysite`; the profile and the R5 cross-version canonical write `bodySite`.
const BODY_SITE: &str =
    "http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.bodySite";

/// The implicit system of `Attachment.contentType`, a code bound to the
/// BCP 13 media types (FHIR R4 `mimetypes`).
const MEDIA_TYPES: &str = "urn:ietf:bcp:13";

/// The implicit system of `DocumentReference.status` (FHIR R4
/// `document-reference-status`).
const STATUS: &str = "http://hl7.org/fhir/document-reference-status";

/// Why a search parameter value is refused, naming no value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CriterionError {
    /// A token value of the parameter `name` is outside the token grammar.
    #[error("a {name} value is not a token of the form [system|]code")]
    Token {
        /// The parameter.
        name: &'static str,
        /// Why the value is refused.
        #[source]
        source: TokenError,
    },
    /// A date value of the parameter `name` is outside the date grammar or
    /// the calendar.
    #[error("a {name} value is not a date with an optional prefix")]
    Date {
        /// The parameter.
        name: &'static str,
        /// Why the value is refused.
        #[source]
        source: DateError,
    },
}

/// One parameter the gateway evaluates, with the values one occurrence ORs.
#[derive(Clone, PartialEq, Eq)]
enum Criterion {
    /// `category`: `DocumentReference.category`.
    Category(Vec<Token>),
    /// `contenttype`: `DocumentReference.content.attachment.contentType`.
    ContentType(Vec<Token>),
    /// `status`: `DocumentReference.status`.
    Status(Vec<Token>),
    /// `modality`: the R5 `modality` extension's `CodeableConcept`.
    Modality(Vec<Token>),
    /// `bodysite`: the R5 `bodySite` extension's `concept`.
    BodySite(Vec<Token>),
    /// `creation`: `DocumentReference.content.attachment.creation`.
    Creation(Vec<DateValue>),
    /// `date`: `DocumentReference.date`.
    Date(Vec<DateValue>),
    /// `period`: `DocumentReference.context.period`.
    Period(Vec<DateValue>),
}

/// Every evaluated parameter a document search gives.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Criteria {
    criteria: Vec<Criterion>,
}

impl std::fmt::Debug for Criteria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Criteria")
            .field("criteria", &self.criteria.len())
            .finish()
    }
}

impl Criteria {
    /// Reads the parameter `name` with the value `value` into the criteria.
    ///
    /// Returns `Ok(false)` when `name` is no parameter the gateway
    /// evaluates, so the caller decides what it means.
    ///
    /// # Errors
    ///
    /// A [`CriterionError`] when a value is outside its parameter's grammar.
    pub fn read(&mut self, name: &str, value: &str) -> Result<bool, CriterionError> {
        let criterion = match name {
            "category" => Criterion::Category(tokens(value, "category")?),
            "contenttype" => Criterion::ContentType(tokens(value, "contenttype")?),
            "status" => Criterion::Status(tokens(value, "status")?),
            "modality" => Criterion::Modality(tokens(value, "modality")?),
            "bodysite" => Criterion::BodySite(tokens(value, "bodysite")?),
            "creation" => Criterion::Creation(dates(value, "creation")?),
            "date" => Criterion::Date(dates(value, "date")?),
            "period" => Criterion::Period(dates(value, "period")?),
            _ => return Ok(false),
        };
        self.criteria.push(criterion);
        Ok(true)
    }

    /// Returns whether `document`, a `DocumentReference` as JSON, meets
    /// every criterion, the instant `now` deciding an `ap` date.
    #[must_use]
    pub fn selects(&self, document: &Value, now: Timestamp) -> bool {
        self.criteria
            .iter()
            .all(|criterion| criterion.selects(document, now))
    }
}

impl Criterion {
    /// Whether `document` meets one of the values.
    fn selects(&self, document: &Value, now: Timestamp) -> bool {
        match self {
            Self::Category(tokens) => {
                concepts(items(document, "category")).any(|coding| coded(tokens, coding))
            }
            Self::ContentType(tokens) => attachments(document)
                .filter_map(|attachment| attachment.get("contentType")?.as_str())
                .any(|media| {
                    tokens
                        .iter()
                        .any(|token| code_matches(token, MEDIA_TYPES, media, true))
                }),
            Self::Status(tokens) => {
                document
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| {
                        tokens
                            .iter()
                            .any(|token| code_matches(token, STATUS, status, false))
                    })
            }
            Self::Modality(tokens) => concepts(
                extensions(document, MODALITY).filter_map(|e| e.get("valueCodeableConcept")),
            )
            .any(|coding| coded(tokens, coding)),
            Self::BodySite(tokens) => concepts(
                extensions(document, BODY_SITE)
                    .flat_map(|site| items(site, "extension"))
                    .filter(|e| e.get("url").and_then(Value::as_str) == Some("concept"))
                    .filter_map(|e| e.get("valueCodeableConcept")),
            )
            .any(|coding| coded(tokens, coding)),
            Self::Creation(values) => attachments(document)
                .filter_map(|attachment| attachment.get("creation")?.as_str())
                .filter_map(point)
                .any(|target| values.iter().any(|value| dated(value, target, now))),
            Self::Date(values) => document
                .get("date")
                .and_then(Value::as_str)
                .and_then(point)
                .is_some_and(|target| values.iter().any(|value| dated(value, target, now))),
            Self::Period(values) => document
                .get("context")
                .and_then(|context| context.get("period"))
                .and_then(period)
                .is_some_and(|target| values.iter().any(|value| dated(value, target, now))),
        }
    }
}

/// The token values of one occurrence of `name`.
fn tokens(value: &str, name: &'static str) -> Result<Vec<Token>, CriterionError> {
    split(value, ',')
        .iter()
        .map(|part| Token::parse(part).map_err(|source| CriterionError::Token { name, source }))
        .collect()
}

/// The date values of one occurrence of `name`.
fn dates(value: &str, name: &'static str) -> Result<Vec<DateValue>, CriterionError> {
    split(value, ',')
        .iter()
        .map(|part| DateValue::parse(part).map_err(|source| CriterionError::Date { name, source }))
        .collect()
}

/// The members of the array `name` of `value`, none when it has none.
fn items<'a>(value: &'a Value, name: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(name)
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
}

/// Every `content.attachment` of `document`.
fn attachments(document: &Value) -> impl Iterator<Item = &Value> {
    items(document, "content").filter_map(|content| content.get("attachment"))
}

/// Every extension of `document` with the URL `url`.
fn extensions<'a>(document: &'a Value, url: &'a str) -> impl Iterator<Item = &'a Value> {
    items(document, "extension")
        .filter(move |extension| extension.get("url").and_then(Value::as_str) == Some(url))
}

/// Every coding of the `CodeableConcept`s `concepts`.
fn concepts<'a>(concepts: impl Iterator<Item = &'a Value>) -> impl Iterator<Item = &'a Value> {
    concepts.flat_map(|concept| items(concept, "coding"))
}

/// Whether the `Coding` `coding` meets one of `tokens` (§3.1.1.4.10).
fn coded(tokens: &[Token], coding: &Value) -> bool {
    let system = coding.get("system").and_then(Value::as_str);
    let code = coding.get("code").and_then(Value::as_str);
    tokens.iter().any(|token| match token {
        Token::Code(wanted) => code == Some(wanted.as_str()),
        Token::SystemCode {
            system: wanted_system,
            code: wanted,
        } => system == Some(wanted_system.as_str()) && code == Some(wanted.as_str()),
        Token::NoSystem(wanted) => system.is_none() && code == Some(wanted.as_str()),
        Token::System(wanted_system) => system == Some(wanted_system.as_str()),
    })
}

/// Whether the `code` element `code`, whose system is the implicit
/// `system` of its binding, meets `token` (§3.1.1.4.10), ignoring ASCII
/// case when `folded`.
fn code_matches(token: &Token, system: &str, code: &str, folded: bool) -> bool {
    let same = |wanted: &str| {
        if folded {
            wanted.eq_ignore_ascii_case(code)
        } else {
            wanted == code
        }
    };
    match token {
        Token::Code(wanted) => same(wanted),
        Token::SystemCode {
            system: wanted_system,
            code: wanted,
        } => wanted_system == system && same(wanted),
        Token::NoSystem(_) => false,
        Token::System(wanted_system) => wanted_system == system,
    }
}

/// The span `[from, until)` a target element covers on the UTC time line,
/// each end open when `None`.
#[derive(Debug, Clone, Copy)]
struct Span {
    from: Option<Timestamp>,
    until: Option<Timestamp>,
}

/// The span of a FHIR `instant`, `dateTime` or `date` at its precision.
// NOTE: FHIR R4 search §3.1.1.4.7: a time without an offset is read in the server's zone, and
// the gateway's is UTC; an element outside the date form spans nothing and matches no value.
fn point(text: &str) -> Option<Span> {
    let instants = SearchDate::parse(text)
        .ok()?
        .instants(&TimeZone::UTC)
        .ok()?;
    Some(Span {
        from: Some(instants.from()),
        until: instants.until(),
    })
}

/// The span of a FHIR `Period`: from its start's first instant to the end
/// of its end, an absent end open (FHIR R4 `Period`).
fn period(period: &Value) -> Option<Span> {
    let start = period.get("start").and_then(Value::as_str);
    let end = period.get("end").and_then(Value::as_str);
    if start.is_none() && end.is_none() {
        return None;
    }
    let from = match start {
        Some(start) => Some(point(start)?.from?),
        None => None,
    };
    let until = match end {
        Some(end) => point(end)?.until,
        None => None,
    };
    Some(Span { from, until })
}

/// Whether `target` meets the date value `value` by its prefix
/// (§3.1.1.4.5), `now` deciding an `ap` margin.
fn dated(value: &DateValue, target: Span, now: Timestamp) -> bool {
    let Ok(search) = value.date().instants(&TimeZone::UTC) else {
        return false;
    };
    // NOTE: an open end lies before or after every instant.
    let starts_before = |a: Option<Timestamp>, b: Timestamp| a.is_none_or(|a| a < b);
    let ends_after = |a: Option<Timestamp>, b: Option<Timestamp>| match (a, b) {
        (None, Some(_)) => true,
        (_, None) => false,
        (Some(a), Some(b)) => a > b,
    };
    let contained =
        !starts_before(target.from, search.from()) && !ends_after(target.until, search.until());
    match value.prefix() {
        Prefix::Eq => contained,
        Prefix::Ne => !contained,
        Prefix::Gt => ends_after(target.until, search.until()),
        Prefix::Lt => starts_before(target.from, search.from()),
        Prefix::Ge => contained || ends_after(target.until, search.until()),
        Prefix::Le => contained || starts_before(target.from, search.from()),
        Prefix::Sa => search
            .until()
            .is_some_and(|until| target.from.is_some_and(|from| from >= until)),
        Prefix::Eb => target.until.is_some_and(|until| until <= search.from()),
        Prefix::Ap => value
            .date()
            .approximate(&TimeZone::UTC, now)
            .is_ok_and(|margin| overlaps(target, margin)),
    }
}

/// Whether `target` overlaps the instants `range`.
fn overlaps(target: Span, range: Instants) -> bool {
    let before_end = match (target.from, range.until()) {
        (Some(from), Some(until)) => from < until,
        _ => true,
    };
    let after_start = target.until.is_none_or(|until| until > range.from());
    before_end && after_start
}

#[cfg(test)]
mod tests {
    use super::Criteria;
    use fhir_types::codec::Value;
    use jiff::Timestamp;

    fn document() -> Value {
        Value::from_serde_json(serde_json::json!({
            "resourceType": "DocumentReference",
            "status": "current",
            "date": "2026-10-21T08:00:00Z",
            "category": [{"coding": [{"system": "http://loinc.org", "code": "18719-5"}]}],
            "extension": [
                {
                    "url": "http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.modality",
                    "valueCodeableConcept": {"coding": [{"system": "http://dicom.nema.org/resources/ontology/DCM", "code": "MR"}]}
                },
                {
                    "url": "http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.bodySite",
                    "extension": [{"url": "concept", "valueCodeableConcept": {"coding": [{"system": "http://snomed.info/sct", "code": "72696002"}]}}]
                }
            ],
            "content": [{"attachment": {"contentType": "application/pdf", "creation": "2026-10-20"}}],
            "context": {"period": {"start": "2026-10-01", "end": "2026-10-05"}}
        }))
    }

    fn selects(query: &[(&str, &str)]) -> bool {
        let mut criteria = Criteria::default();
        for (name, value) in query {
            assert_eq!(criteria.read(name, value), Ok(true), "{name}");
        }
        criteria.selects(&document(), Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn each_parameter_selects_the_document_it_describes() {
        for query in [
            [("category", "http://loinc.org|18719-5")],
            [("category", "18719-5,other")],
            [("contenttype", "application/PDF")],
            [("contenttype", "urn:ietf:bcp:13|application/pdf")],
            [("status", "current")],
            [("modality", "MR")],
            [("bodysite", "http://snomed.info/sct|72696002")],
            [("creation", "2026-10")],
            [("creation", "ge2026-10-20")],
            [("date", "2026-10-21")],
            [("date", "lt2026-10-22")],
            [("period", "2026-10")],
            [("period", "sa2026-09-30")],
            [("period", "eb2026-10-06")],
        ] {
            assert!(selects(&query), "{query:?}");
        }
    }

    #[test]
    fn each_parameter_excludes_a_document_it_does_not_describe() {
        for query in [
            [("category", "http://snomed.info/sct|18719-5")],
            [("contenttype", "image/png")],
            [("contenttype", "|application/pdf")],
            [("status", "superseded")],
            [("modality", "CT")],
            [("bodysite", "1")],
            [("creation", "2026-11")],
            [("date", "gt2026-10-21")],
            [("period", "2026-10-03")],
            [("period", "sa2026-10-01")],
            [("date", "ne2026-10-21")],
        ] {
            assert!(!selects(&query), "{query:?}");
        }
    }

    #[test]
    fn repetitions_are_anded() {
        assert!(!selects(&[("status", "current"), ("modality", "CT")]));
        assert!(selects(&[("status", "current"), ("modality", "MR")]));
    }

    #[test]
    fn a_document_without_the_element_matches_no_value() {
        let mut criteria = Criteria::default();
        assert_eq!(criteria.read("period", "2026"), Ok(true));
        let bare = Value::from_serde_json(serde_json::json!({"resourceType": "DocumentReference"}));
        assert!(!criteria.selects(&bare, Timestamp::UNIX_EPOCH));
    }

    #[test]
    fn a_malformed_value_is_refused_and_an_unknown_name_is_not_read() {
        let mut criteria = Criteria::default();
        assert!(criteria.read("date", "2026-13").is_err());
        assert!(criteria.read("status", "|").is_err());
        assert_eq!(criteria.read("subject", "x"), Ok(false));
    }
}
