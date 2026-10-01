// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient a façade query identifies, and the paths that carry it.
//!
//! The `EHR_STATUS.subject.external_ref` carrier is the subject predicate of
//! §7: `<ehr>/ehr_status/subject/external_ref/id/value = <patientId>`, with
//! the issuing namespace in `<ehr>/ehr_status/subject/external_ref/namespace`
//! (§5.2, §5.4.3).

use std::fmt;

use openehr_query::ast::{IdentifiedPath, PathPart};

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
    /// The query predicated on `…/external_ref/namespace` itself.
    Query,
    /// The query named none, and the deployment declares a default issuing
    /// namespace (decision A5; FerroFED's own reading of §5.2, which requires
    /// the namespace and does not say where an unqualified one comes from).
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

/// What a path over `EHR_STATUS.subject` is, for the rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SubjectPath {
    /// `<ehr>/ehr_status/subject/external_ref/id/value`: the identifier.
    Id,
    /// `<ehr>/ehr_status/subject/external_ref/namespace`: its namespace.
    Namespace,
    /// Any other path under `ehr_status/subject`, or one of the two above
    /// written with a predicate or on a variable that is not an `EHR`.
    Other,
}

const ID: [&str; 5] = ["ehr_status", "subject", "external_ref", "id", "value"];
const NAMESPACE: [&str; 4] = ["ehr_status", "subject", "external_ref", "namespace"];

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

/// Whether `path` reaches the identifiers of a `subject` that is not the
/// `EHR_STATUS` one: an `ENTRY`-level `subject` (`PARTY_IDENTIFIED` or
/// `PARTY_RELATED`, §5.4.2).
pub(super) fn entry_subject_path(path: &IdentifiedPath, ehr: &[String]) -> bool {
    if ehr.contains(&path.root) {
        return false;
    }
    let parts = path.path.as_ref().map_or(&[][..], |p| p.parts.as_slice());
    parts.windows(2).any(|pair| match pair {
        [subject, next] => {
            subject.name == "subject"
                && matches!(next.name.as_str(), "identifiers" | "external_ref")
        }
        _ => false,
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
