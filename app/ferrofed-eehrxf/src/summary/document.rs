// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member's own patient summary, read for what the face needs of it: that
//! it is a FHIR document `Bundle`, and the system and value of its
//! `Bundle.identifier` (FHIR R4 `bdl-9`).
//!
//! The engine forwards the member's bytes unchanged and reads none of them;
//! this reader walks them as the `fhir-types` value tree by the element
//! names, so an element FHIR R4 does not define, or one the face does not
//! read, never refuses the document. An element it reads that holds the
//! wrong kind of JSON value refuses it, naming the element. A `null` reads
//! as absent (FHIR R4 `json.html`). No specification governs the reader:
//! our own design.

use fhir_types::codec::Value;

/// Why a member's success is no document; no variant's text names a value
/// the answer carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unusable {
    /// The answer is no JSON.
    #[error("the answer is no FHIR resource in JSON")]
    Unreadable(#[source] serde_json::Error),
    /// An element the face reads holds another kind of JSON value than
    /// FHIR R4 gives it.
    #[error("the answer's {element} is of the wrong JSON kind")]
    Kind {
        /// The element, such as `Bundle.identifier`.
        element: &'static str,
    },
    /// The answer is a resource other than a `Bundle`.
    #[error("the answer is no Bundle")]
    NotBundle,
    /// The `Bundle` is of a type other than `document`.
    #[error("the Bundle is no document")]
    NotDocument,
    /// The document carries no identifier with a system and a value
    /// (FHIR R4 `bdl-9`).
    #[error("the document carries no identifier with a system and a value (bdl-9)")]
    NoIdentifier,
}

/// The system and the value of the `Bundle.identifier` of `body`, a member's
/// answer, once it reads as a FHIR document `Bundle`.
///
/// # Errors
///
/// The [`Unusable`] that says why `body` is no FHIR document `Bundle` whose
/// identifier has a system and a value, which FHIR R4 `bdl-9` requires of
/// every document.
pub fn identifier(body: &[u8]) -> Result<(String, String), Unusable> {
    let resource: Value = serde_json::from_slice(body).map_err(Unusable::Unreadable)?;
    if resource.as_object().is_none() {
        return Err(Unusable::Kind { element: "Bundle" });
    }
    if text(&resource, "resourceType", "Bundle.resourceType")? != Some("Bundle") {
        return Err(Unusable::NotBundle);
    }
    if text(&resource, "type", "Bundle.type")? != Some("document") {
        return Err(Unusable::NotDocument);
    }
    let identifier = match resource.get("identifier") {
        None | Some(Value::Null) => return Err(Unusable::NoIdentifier),
        Some(identifier) if identifier.as_object().is_some() => identifier,
        Some(_) => {
            return Err(Unusable::Kind {
                element: "Bundle.identifier",
            });
        }
    };
    let system = text(identifier, "system", "Bundle.identifier.system")?;
    let value = text(identifier, "value", "Bundle.identifier.value")?;
    match (system, value) {
        (Some(system), Some(value)) if !system.is_empty() && !value.is_empty() => {
            Ok((system.to_owned(), value.to_owned()))
        }
        _ => Err(Unusable::NoIdentifier),
    }
}

/// The string member `name` of `held`, `None` when it is absent or `null`.
fn text<'a>(
    held: &'a Value,
    name: &str,
    element: &'static str,
) -> Result<Option<&'a str>, Unusable> {
    match held.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(member) => member.as_str().map(Some).ok_or(Unusable::Kind { element }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Unusable, identifier};

    const IDENTIFIED: &str =
        "\"identifier\":{\"system\":\"urn:oid:2.999.4.3\",\"value\":\"synthetic-doc-x\"}";

    #[test]
    fn a_document_bundle_gives_its_identifier_and_any_other_element_is_left_unread() {
        let body = format!(
            "{{\"resourceType\":\"Bundle\",\"type\":\"document\",{IDENTIFIED},\"entry\":[],\"unknownToR4\":{{\"a\":1}}}}"
        );
        let read = identifier(body.as_bytes()).expect("a document");
        assert_eq!(
            ("urn:oid:2.999.4.3".to_owned(), "synthetic-doc-x".to_owned()),
            read
        );
    }

    #[test]
    fn a_document_with_no_identifier_is_unusable() {
        for body in [
            "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"entry\":[]}",
            "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":null}",
        ] {
            assert!(
                matches!(identifier(body.as_bytes()), Err(Unusable::NoIdentifier)),
                "{body}"
            );
        }
    }

    #[test]
    fn a_document_whose_identifier_lacks_a_system_or_a_value_is_unusable() {
        for body in [
            "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"value\":\"synthetic-doc-x\"}}",
            "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":\"urn:oid:2.999.4.3\"}}",
            "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":\"\",\"value\":\"synthetic-doc-x\"}}",
        ] {
            assert!(
                matches!(identifier(body.as_bytes()), Err(Unusable::NoIdentifier)),
                "{body}"
            );
        }
    }

    #[test]
    fn an_answer_that_is_no_json_is_unreadable() {
        assert!(matches!(
            identifier(b"synthetic text that is no FHIR resource"),
            Err(Unusable::Unreadable(_))
        ));
    }

    #[test]
    fn an_element_of_the_wrong_json_kind_is_refused_naming_it() {
        for (body, named) in [
            ("[]", "Bundle"),
            (
                "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":[]}",
                "Bundle.identifier",
            ),
            (
                "{\"resourceType\":7,\"type\":\"document\"}",
                "Bundle.resourceType",
            ),
            ("{\"resourceType\":\"Bundle\",\"type\":true}", "Bundle.type"),
            (
                "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":1,\"value\":\"v\"}}",
                "Bundle.identifier.system",
            ),
        ] {
            assert!(
                matches!(identifier(body.as_bytes()), Err(Unusable::Kind { element }) if element == named),
                "{body}"
            );
        }
    }

    #[test]
    fn a_resource_other_than_a_document_bundle_is_unusable() {
        let patient = format!("{{\"resourceType\":\"Patient\",{IDENTIFIED}}}");
        assert!(matches!(
            identifier(patient.as_bytes()),
            Err(Unusable::NotBundle)
        ));
        let collection =
            format!("{{\"resourceType\":\"Bundle\",\"type\":\"collection\",{IDENTIFIED}}}");
        assert!(matches!(
            identifier(collection.as_bytes()),
            Err(Unusable::NotDocument)
        ));
    }

    #[test]
    fn no_refusal_names_a_value_the_answer_carries() {
        let body = "{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":\"Qz7sys\",\"value\":7}}";
        let refused = identifier(body.as_bytes()).expect_err("a value of the wrong kind");
        let shown = format!("{refused} {refused:?}");
        assert!(!shown.contains("Qz7sys"), "{shown}");
    }
}
