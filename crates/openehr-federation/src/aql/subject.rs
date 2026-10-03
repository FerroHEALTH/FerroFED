// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient a façade query identifies, and the paths that carry it.
//!
//! A gateway accepts the patient identifier in either carrier and resolves on
//! whichever the client used (§5.4.3, N33, CP-38):
//!
//! - `EHR_STATUS.subject.external_ref`, the subject predicate of §7:
//!   `<ehr>/ehr_status/subject/external_ref/id/value = <patientId>`, with the
//!   issuing namespace in `<ehr>/ehr_status/subject/external_ref/namespace`;
//! - an `ENTRY`-level `subject` (`PARTY_IDENTIFIED` / `DV_IDENTIFIER`):
//!   `…/subject/identifiers/id = <patientId>`, with the issuing namespace in
//!   `…/subject/identifiers/issuer` or `…/subject/identifiers/type`.

use std::fmt;

use openehr_query::ast::{IdentifiedPath, PathPart};

use super::Context;
use super::refusal::{Refusal, Unreducible};
use super::scan::{Findings, Input};

/// The patient a façade query identifies: the identifier and its issuing
/// namespace (§5.2), consumed at the gateway as resolution input.
///
/// The identifier is a directly identifying value (§5.1), so `Debug` redacts
/// it and the type has no `Display`: it reaches a log or an error only by a
/// deliberate call to [`Subject::value`] (§5.4.3, N33).
#[derive(Clone, PartialEq, Eq)]
pub struct Subject {
    value: String,
    namespace: String,
    origin: NamespaceOrigin,
}

/// Where the issuing namespace of a [`Subject`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceOrigin {
    /// The query named the namespace itself: `…/external_ref/namespace`, or
    /// the `issuer` or `type` of an `ENTRY`-level `DV_IDENTIFIER` (§5.4.3).
    Query,
    /// The query named none, and the deployment declares a default issuing
    /// namespace (§5.2 requires the namespace and does not say where an
    /// unqualified one comes from; no specification governs this: our own
    /// design).
    Default,
}

impl Subject {
    pub(super) fn new(value: String, namespace: String, origin: NamespaceOrigin) -> Self {
        Self {
            value,
            namespace,
            origin,
        }
    }

    /// The patient identifier, for the cross-reference lookup of §5.2.
    ///
    /// It is the one value the gateway must never dispatch to a node, log or
    /// echo in an error (§5.4.1, N33).
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The namespace that issued the identifier (§5.2).
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Whether the namespace came from the query or from the declared default.
    #[must_use]
    pub fn namespace_origin(&self) -> NamespaceOrigin {
        self.origin
    }
}

impl fmt::Debug for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subject")
            .field("value", &"[REDACTED]")
            .field("namespace", &self.namespace)
            .field("origin", &self.origin)
            .finish()
    }
}

/// What a path over a patient carrier is, for the rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubjectPath {
    /// The identifier: `<ehr>/ehr_status/subject/external_ref/id/value`, or
    /// `…/subject/identifiers/id` of an `ENTRY`.
    Id,
    /// Its issuing namespace: `<ehr>/ehr_status/subject/external_ref/namespace`,
    /// or `…/subject/identifiers/issuer` or `…/subject/identifiers/type` of an
    /// `ENTRY` (§5.4.3).
    Namespace,
    /// Any other path into a patient carrier, or one of the above written with
    /// a predicate on the carrier or on a variable that cannot carry it.
    Other,
}

const ID: [&str; 5] = ["ehr_status", "subject", "external_ref", "id", "value"];
const NAMESPACE: [&str; 4] = ["ehr_status", "subject", "external_ref", "namespace"];

/// Classifies `path` when it reaches into either patient carrier, or `None`.
///
/// `ehr` lists the variables the `FROM` clause binds to the `EHR` class.
pub(super) fn patient_path(path: &IdentifiedPath, ehr: &[String]) -> Option<SubjectPath> {
    subject_path(path, ehr).or_else(|| entry_path(path, ehr))
}

/// Classifies `path` when it reaches into `EHR_STATUS.subject`, or `None`.
///
/// `ehr` lists the variables the `FROM` clause binds to the `EHR` class. A
/// subject-shaped path on any other root is [`SubjectPath::Other`]: it cannot
/// be consumed, and it is never ordinary query material either.
pub(super) fn subject_path(path: &IdentifiedPath, ehr: &[String]) -> Option<SubjectPath> {
    let parts = path.path.as_ref().map_or(&[][..], |p| p.parts.as_slice());
    if !starts_with(parts, &["ehr_status", "subject"]) {
        return None;
    }
    let exact = ehr.contains(&path.root)
        && path.predicate.is_none()
        && parts.iter().all(|part| part.predicate.is_none());
    Some(if exact && names_are(parts, &ID) {
        SubjectPath::Id
    } else if exact && names_are(parts, &NAMESPACE) {
        SubjectPath::Namespace
    } else {
        SubjectPath::Other
    })
}

/// Classifies `path` when it reaches into an `ENTRY`-level `subject`, or
/// `None`.
///
/// The carrier is `…/subject/identifiers/<attribute>` on any root that is not
/// an `EHR` variable: `ENTRY.subject` is a `PARTY_PROXY`, and
/// `PARTY_IDENTIFIED` holds the `DV_IDENTIFIER` list (§5.4.2). `id` is the
/// identifier and `issuer` or `type` its namespace (§5.4.3). The navigation
/// before `subject` may carry predicates; the carrier itself may not, so
/// `…/subject/identifiers[…]/id`, `assigner`, and `…/subject/external_ref` are
/// [`SubjectPath::Other`], refused rather than guessed at.
fn entry_path(path: &IdentifiedPath, ehr: &[String]) -> Option<SubjectPath> {
    if ehr.contains(&path.root) {
        return None;
    }
    let parts = path.path.as_ref().map_or(&[][..], |p| p.parts.as_slice());
    let start = parts.windows(2).position(|pair| match pair {
        [subject, next] => {
            subject.name == "subject"
                && matches!(next.name.as_str(), "identifiers" | "external_ref")
        }
        _ => false,
    })?;
    let carrier = parts.get(start..).unwrap_or_default();
    if carrier.iter().any(|part| part.predicate.is_some()) {
        return Some(SubjectPath::Other);
    }
    Some(
        match carrier
            .iter()
            .map(|part| part.name.as_str())
            .collect::<Vec<_>>()
            .as_slice()
        {
            ["subject", "identifiers", "id"] => SubjectPath::Id,
            ["subject", "identifiers", "issuer" | "type"] => SubjectPath::Namespace,
            _ => SubjectPath::Other,
        },
    )
}

/// Whether `path` reaches an identifier the RM keeps as a value or a
/// reference: a `PARTY_IDENTIFIED.identifiers` list (`DV_IDENTIFIER`) or a
/// `PARTY_REF` (`external_ref`), on any subject, composer, performer, facility
/// or committer (§5.4.2).
pub(super) fn identifier_bearing(path: &IdentifiedPath) -> bool {
    path.path.as_ref().is_some_and(|p| {
        p.parts
            .iter()
            .any(|part| matches!(part.name.as_str(), "identifiers" | "external_ref"))
    })
}

/// Whether `path` is `<ehr>/ehr_id/value`, the canonical `ehr_id` scope of
/// N29.
pub(super) fn ehr_id_path(path: &IdentifiedPath, ehr: &[String]) -> bool {
    let parts = path.path.as_ref().map_or(&[][..], |p| p.parts.as_slice());
    ehr.contains(&path.root)
        && path.predicate.is_none()
        && names_are(parts, &["ehr_id", "value"])
        && parts.iter().all(|part| part.predicate.is_none())
}

fn names_are(parts: &[PathPart], names: &[&str]) -> bool {
    parts.len() == names.len()
        && parts
            .iter()
            .zip(names)
            .all(|(part, name)| part.name == *name)
}

fn starts_with(parts: &[PathPart], names: &[&str]) -> bool {
    parts.len() >= names.len()
        && parts
            .iter()
            .zip(names)
            .all(|(part, name)| part.name == *name)
}

/// The patient the query names and the top-level leaves that name it, or
/// `None` for a query without a patient predicate.
pub(super) fn named(
    findings: &Findings,
    context: &Context,
) -> Result<Option<(Subject, Vec<usize>)>, Refusal> {
    let Some(first) = findings.ids.first() else {
        if let Some((_, _, at)) = findings
            .inputs
            .iter()
            .find(|(_, input, _)| *input == Input::Id)
        {
            return Err(Refusal::SubjectWithoutPredicate { at: at.clone() });
        }
        return Ok(None);
    };
    if let Some(second) = findings.ids.iter().find(|found| found.value != first.value) {
        return Err(Refusal::SecondSubject {
            at: second.at.clone(),
        });
    }
    if first.value.is_empty() {
        return Err(Refusal::EmptyIdentifier {
            at: first.at.clone(),
        });
    }
    if findings.ehr.len() > 1 {
        return Err(Refusal::Unreducible {
            reason: Unreducible::SeveralEhrs,
            at: first.at.clone(),
        });
    }
    let namespace = match findings.namespaces.first() {
        Some(named) => {
            if let Some(second) = findings
                .namespaces
                .iter()
                .find(|found| found.value != named.value)
            {
                return Err(Refusal::SecondNamespace {
                    at: second.at.clone(),
                });
            }
            (named.value.clone(), NamespaceOrigin::Query)
        }
        None => match &context.default_namespace {
            Some(default) => (default.clone(), NamespaceOrigin::Default),
            None => return Err(Refusal::NoNamespace),
        },
    };
    let consumed = findings
        .ids
        .iter()
        .chain(&findings.namespaces)
        .map(|found| found.leaf)
        .collect();
    let (namespace, origin) = namespace;
    Ok(Some((
        Subject::new(first.value.clone(), namespace, origin),
        consumed,
    )))
}

#[cfg(test)]
mod tests {
    use super::{NamespaceOrigin, Subject};

    #[test]
    fn debug_never_shows_the_identifier() {
        let subject = Subject::new(
            "sentinel-6491".into(),
            "urn:oid:2.999.1".into(),
            NamespaceOrigin::Query,
        );
        let shown = format!("{subject:?}");
        assert!(
            !shown.contains("sentinel-6491"),
            "Debug echoed the identifier: {shown}"
        );
        assert!(
            shown.contains("urn:oid:2.999.1"),
            "Debug should still name the namespace: {shown}"
        );
    }
}
