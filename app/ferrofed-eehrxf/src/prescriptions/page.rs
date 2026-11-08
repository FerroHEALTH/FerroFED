// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One page of a member's own prescription search, read for what the face
//! passes through.
//!
//! A page is a FHIR R4 `Bundle` of type `searchset` (`http.html` §3.1.0.9).
//! Its `match` entries are the member's prescriptions, each a
//! `MedicationRequest` kept as the member's JSON value, every number in the
//! text the member wrote (RFC 8259 §4 gives member order no meaning); its
//! `include` and `outcome` entries are kept the same way; and its `next`
//! link names the member's following page (§3.1.0.14). An entry with no
//! search mode is a match when it is a `MedicationRequest`, and an include
//! otherwise (FHIR R4 Bundle `entry.search.mode`, 0..1). A page that is no
//! `searchset`, or a match that is no `MedicationRequest`, says nothing a
//! client could read as the member's prescriptions, so it is [`Unusable`],
//! never an empty page.

use fhir_types::codec::{Object, Value};
use fhir_types::r4::resource::UnknownResource;
use url::Url;

/// A resource of a member's page, its JSON value as the member sent it,
/// under the entry's `fullUrl` when the member gave one.
///
/// `Debug` shows the resource's type alone.
#[derive(Clone)]
pub struct Kept {
    /// The entry's `fullUrl`, as the member gave it.
    pub full_url: Option<String>,
    /// The resource, as the member sent it.
    pub resource: Value,
    /// The resource's type.
    pub resource_type: String,
}

impl Kept {
    /// Returns the resource's logical id, when it carries one as a string.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.resource.get("id").and_then(Value::as_str)
    }

    /// Names the resource by the logical id `id`, the one change made to it;
    /// a resource that is no JSON object is left as it is.
    pub fn rename(&mut self, id: &str) {
        if let Value::Object(object) = &mut self.resource {
            object.insert("id".to_owned(), Value::String(id.to_owned()));
        }
    }

    /// Returns the resource as the generated `Resource::Unknown`, its JSON
    /// value as kept.
    #[must_use]
    pub fn unknown(&self) -> fhir_types::r4::resource::Resource {
        fhir_types::r4::resource::Resource::Unknown(UnknownResource {
            resource_type: self.resource_type.clone(),
            body: self.resource.clone(),
        })
    }
}

impl std::fmt::Debug for Kept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kept")
            .field("resource_type", &self.resource_type)
            .finish_non_exhaustive()
    }
}

/// One prescription of a member's page, with the business identifiers it
/// carries, each a system and a value (`MedicationRequest.identifier`).
///
/// `Debug` shows how many identifiers it carries, never one.
#[derive(Clone)]
pub struct Prescription {
    /// The resource, as the member sent it.
    pub kept: Kept,
    /// Its identifiers that have both a system and a value.
    pub identifiers: Vec<(String, String)>,
    /// Whether the face can give it no id to read it by, so it is listed
    /// as its member states it and cannot be read by id.
    pub unreadable: bool,
}

impl std::fmt::Debug for Prescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prescription")
            .field("identifiers", &self.identifiers.len())
            .finish_non_exhaustive()
    }
}

/// One page of a member's search, read.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// The prescriptions, in the member's order.
    pub prescriptions: Vec<Prescription>,
    /// The resources the member included, in its order.
    pub included: Vec<Kept>,
    /// The `OperationOutcome`s the member said about its search, in its
    /// order.
    pub outcomes: Vec<Kept>,
    /// The member's `next` link, resolved against its base.
    pub next: Option<Url>,
}

/// Why a member's success is no page of prescriptions; no variant's text
/// names a value the page carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unusable {
    /// The answer is no JSON object of a FHIR resource's shape.
    #[error("the answer is no FHIR resource in JSON")]
    Unreadable(#[source] serde_json::Error),
    /// The answer is a resource other than a `Bundle`.
    #[error("the answer is no Bundle")]
    NotBundle,
    /// An element the face reads holds a JSON value of another kind than
    /// its FHIR JSON form (FHIR R4 `json.html`).
    #[error("the searchset's {0} is not of its FHIR JSON form")]
    Shape(&'static str),
    /// The `Bundle` is of a type other than `searchset`.
    #[error("the Bundle is no searchset")]
    NotSearchset,
    /// An entry carries no resource.
    #[error("an entry of the searchset carries no resource")]
    Empty,
    /// A `match` entry is not of the resource type searched.
    #[error("a match of the searchset is not of the resource type searched")]
    Foreign,
    /// An `outcome` entry is no `OperationOutcome`.
    #[error("an outcome entry of the searchset is no OperationOutcome")]
    Outcome,
    /// An entry's search mode is none FHIR R4 defines.
    #[error("an entry of the searchset has a search mode FHIR R4 does not define")]
    Mode,
    /// The `next` link is no URL.
    #[error("the searchset's next link is no URL")]
    Next(#[source] url::ParseError),
}

impl Page {
    /// Reads `body`, a member's answer to a search of the resource type
    /// `matched`, as a page of the member whose base is `base`; each match is
    /// kept in [`Page::prescriptions`] whatever its type is named.
    ///
    /// # Errors
    ///
    /// The [`Unusable`] that says why the body is no page of `matched`.
    pub fn read(body: &[u8], base: &Url, matched: &str) -> Result<Self, Unusable> {
        // NOTE: fhir-types 0.1.109 `codec::Value` reads through `RawValue`, so a number keeps
        // the text the member wrote, which `Value::from_serde_json` would not.
        let Value::Object(mut bundle) =
            serde_json::from_slice::<Value>(body).map_err(Unusable::Unreadable)?
        else {
            return Err(Unusable::NotBundle);
        };
        if text(&bundle, "resourceType", "resourceType")? != Some("Bundle") {
            return Err(Unusable::NotBundle);
        }
        if text(&bundle, "type", "type")? != Some("searchset") {
            return Err(Unusable::NotSearchset);
        }
        let mut page = Self::default();
        for entry in list(bundle.remove("entry"), "entry")? {
            let Value::Object(mut entry) = entry else {
                return Err(Unusable::Shape("entry"));
            };
            let full_url = text(&entry, "fullUrl", "entry.fullUrl")?.map(str::to_owned);
            let mode = match entry.get("search") {
                None | Some(Value::Null) => None,
                Some(Value::Object(search)) => {
                    text(search, "mode", "entry.search.mode")?.map(str::to_owned)
                }
                Some(_) => return Err(Unusable::Shape("entry.search")),
            };
            let resource = match entry.remove("resource") {
                None | Some(Value::Null) => return Err(Unusable::Empty),
                Some(Value::Object(resource)) => resource,
                Some(_) => return Err(Unusable::Shape("entry.resource")),
            };
            let resource_type = text(&resource, "resourceType", "entry.resource.resourceType")?
                .unwrap_or_default()
                .to_owned();
            let prescribed = resource_type == matched;
            let identifiers = identifiers(&resource)?;
            let kept = Kept {
                full_url,
                resource: Value::Object(resource),
                resource_type,
            };
            match (mode.as_deref(), prescribed) {
                (Some("match") | None, true) => page.prescriptions.push(Prescription {
                    kept,
                    identifiers,
                    unreadable: false,
                }),
                (Some("match"), false) => return Err(Unusable::Foreign),
                (Some("include") | None, _) => page.included.push(kept),
                (Some("outcome"), _) if kept.resource_type == "OperationOutcome" => {
                    page.outcomes.push(kept);
                }
                (Some("outcome"), _) => return Err(Unusable::Outcome),
                (Some(_), _) => return Err(Unusable::Mode),
            }
        }
        // NOTE: FHIR R4 `http.html` §3.1.0.14; the links are opaque to the client, so the
        // member's own `next` names its following page, resolved as a URL against its base.
        let mut next = None;
        for link in list(bundle.remove("link"), "link")? {
            let Value::Object(link) = link else {
                return Err(Unusable::Shape("link"));
            };
            let relation = text(&link, "relation", "link.relation")?;
            let url = text(&link, "url", "link.url")?;
            if next.is_none() && relation == Some("next") {
                next = Some(url.map(str::to_owned));
            }
        }
        page.next = next
            .flatten()
            .map(|next| base.join(&next))
            .transpose()
            .map_err(Unusable::Next)?;
        Ok(page)
    }
}

/// The identifiers of a prescription that carry both a system and a value
/// (`MedicationRequest.identifier`, 0..*).
fn identifiers(resource: &Object) -> Result<Vec<(String, String)>, Unusable> {
    let mut found = Vec::new();
    let held = match resource.get("identifier") {
        None | Some(Value::Null) => &[][..],
        Some(Value::Array(items)) => items.as_slice(),
        Some(_) => return Err(Unusable::Shape("entry.resource.identifier")),
    };
    for identifier in held {
        let Value::Object(identifier) = identifier else {
            return Err(Unusable::Shape("entry.resource.identifier"));
        };
        let system = text(identifier, "system", "entry.resource.identifier.system")?;
        let value = text(identifier, "value", "entry.resource.identifier.value")?;
        if let (Some(system), Some(value)) = (system, value)
            && !system.is_empty()
            && !value.is_empty()
        {
            found.push((system.to_owned(), value.to_owned()));
        }
    }
    Ok(found)
}

/// The string `object` holds under `key`, `None` when it is absent or
/// `null`, refused as [`Unusable::Shape`] naming `element` when it is
/// another kind of JSON value.
fn text<'a>(
    object: &'a Object,
    key: &str,
    element: &'static str,
) -> Result<Option<&'a str>, Unusable> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(Unusable::Shape(element)),
    }
}

/// The items of the repeating element `held`, none when it is absent or
/// `null`, refused as [`Unusable::Shape`] naming `element` when it is no
/// array.
fn list(held: Option<Value>, element: &'static str) -> Result<Vec<Value>, Unusable> {
    match held {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items),
        Some(_) => Err(Unusable::Shape(element)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, Unusable};
    use crate::prescriptions::RESOURCE;
    use url::Url;

    fn base() -> Url {
        Url::parse("https://member.example.org/fhir/").unwrap()
    }

    #[test]
    fn a_page_keeps_each_prescription_as_the_member_sent_it() {
        let body = br#"{"resourceType":"Bundle","type":"searchset","link":[{"relation":"next","url":"MedicationRequest?page=2"}],"entry":[
            {"fullUrl":"https://member.example.org/fhir/MedicationRequest/rx-1","resource":{ "resourceType" :"MedicationRequest","id":"rx-1","identifier":[{"system":"urn:oid:2.999.5","value":"rx-1"},{"value":"no-system"}]},"search":{"mode":"match"}},
            {"resource":{"resourceType":"Medication","id":"m-1"},"search":{"mode":"include"}},
            {"resource":{"resourceType":"OperationOutcome","issue":[]},"search":{"mode":"outcome"}}]}"#;
        let page = Page::read(body, &base(), RESOURCE).unwrap();
        let [prescription] = page.prescriptions.as_slice() else {
            panic!("one prescription: {page:?}");
        };
        let sent: fhir_types::codec::Value = serde_json::from_str(
            r#"{ "resourceType" :"MedicationRequest","id":"rx-1","identifier":[{"system":"urn:oid:2.999.5","value":"rx-1"},{"value":"no-system"}]}"#,
        )
        .unwrap();
        assert_eq!(prescription.kept.resource, sent, "the member's value");
        assert_eq!(
            prescription.identifiers,
            [("urn:oid:2.999.5".to_owned(), "rx-1".to_owned())]
        );
        assert_eq!(1, page.included.len());
        assert_eq!(1, page.outcomes.len());
        assert_eq!(
            page.next.unwrap().as_str(),
            "https://member.example.org/fhir/MedicationRequest?page=2"
        );
    }

    #[test]
    fn a_renamed_prescription_changes_its_id_alone() {
        let body = br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationRequest","id":"rx-1","dispenseRequest":{"quantity":{"value":1.50}}}}]}"#;
        let page = Page::read(body, &base(), RESOURCE).unwrap();
        let mut kept = page.prescriptions.first().unwrap().kept.clone();
        assert_eq!(Some("rx-1"), kept.id());
        kept.rename("face-1");
        let expected: fhir_types::codec::Value = serde_json::from_str(
            r#"{"resourceType":"MedicationRequest","id":"face-1","dispenseRequest":{"quantity":{"value":1.50}}}"#,
        )
        .unwrap();
        assert_eq!(expected, kept.resource);
        assert!(
            serde_json::to_string(&kept.unknown())
                .unwrap()
                .contains(":1.50")
        );
    }

    #[test]
    fn an_empty_searchset_is_a_page_with_no_prescription() {
        let page = Page::read(
            br#"{"resourceType":"Bundle","type":"searchset"}"#,
            &base(),
            RESOURCE,
        )
        .unwrap();
        assert!(page.prescriptions.is_empty() && page.next.is_none());
    }

    #[test]
    fn every_other_answer_is_unusable() {
        for (body, expected) in [
            (&b"not json"[..], "Unreadable"),
            (br#"{"resourceType":"OperationOutcome"}"#, "NotBundle"),
            (br#"{"resourceType":"Bundle","type":"collection"}"#, "NotSearchset"),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"search":{"mode":"match"}}]}"#,
                "Empty",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"Patient"},"search":{"mode":"match"}}]}"#,
                "Foreign",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"Patient"},"search":{"mode":"outcome"}}]}"#,
                "Outcome",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationRequest"},"search":{"mode":"other"}}]}"#,
                "Mode",
            ),
        ] {
            let read = Page::read(body, &base(), RESOURCE);
            let name = match read {
                Err(Unusable::Unreadable(_)) => "Unreadable",
                Err(Unusable::NotBundle) => "NotBundle",
                Err(Unusable::NotSearchset) => "NotSearchset",
                Err(Unusable::Empty) => "Empty",
                Err(Unusable::Foreign) => "Foreign",
                Err(Unusable::Outcome) => "Outcome",
                Err(Unusable::Mode) => "Mode",
                other => panic!("{other:?}"),
            };
            assert_eq!(expected, name);
        }
    }

    #[test]
    fn an_element_of_another_json_kind_is_refused_by_its_name() {
        for (body, element) in [
            (&br#"{"resourceType":1}"#[..], "resourceType"),
            (br#"{"resourceType":"Bundle","type":"searchset","entry":{}}"#, "entry"),
            (br#"{"resourceType":"Bundle","type":"searchset","entry":[1]}"#, "entry"),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"fullUrl":2,"resource":{"resourceType":"MedicationRequest"}}]}"#,
                "entry.fullUrl",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":"rx"}]}"#,
                "entry.resource",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"Medication","identifier":[{"value":3}]},"search":{"mode":"include"}}]}"#,
                "entry.resource.identifier.value",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"MedicationRequest"},"search":{"mode":true}}]}"#,
                "entry.search.mode",
            ),
            (
                br#"{"resourceType":"Bundle","type":"searchset","link":[{"relation":"next","url":4}]}"#,
                "link.url",
            ),
        ] {
            match Page::read(body, &base(), RESOURCE) {
                Err(Unusable::Shape(named)) => assert_eq!(element, named),
                other => panic!("{element}: {other:?}"),
            }
        }
    }
}
