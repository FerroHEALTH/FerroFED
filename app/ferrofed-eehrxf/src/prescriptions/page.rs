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

use fhir_types::codec::Value;
use fhir_types::r4::resource::UnknownResource;
use serde::Deserialize;
use serde_json::value::RawValue;
use url::Url;

use super::RESOURCE;

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

/// What a page is read for.
#[derive(Deserialize)]
struct Head {
    /// The resource type.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// The `Bundle.type`.
    #[serde(rename = "type")]
    kind: Option<String>,
    /// The entries.
    #[serde(default)]
    entry: Vec<Entry>,
    /// The links.
    #[serde(default)]
    link: Vec<Link>,
}

/// One entry of a page.
#[derive(Deserialize)]
struct Entry {
    /// The `fullUrl`.
    #[serde(rename = "fullUrl")]
    full_url: Option<String>,
    /// The resource, as the member sent it.
    resource: Option<Box<RawValue>>,
    /// The search information.
    search: Option<Search>,
}

/// An entry's search information.
#[derive(Deserialize)]
struct Search {
    /// The search mode.
    mode: Option<String>,
}

/// One link of a page.
#[derive(Deserialize)]
struct Link {
    /// The relation.
    relation: Option<String>,
    /// The URL.
    url: Option<String>,
}

/// What a resource is read for: its type and its identifiers.
#[derive(Deserialize)]
struct Resource {
    /// The resource type.
    #[serde(rename = "resourceType")]
    resource_type: Option<String>,
    /// The identifiers.
    #[serde(default)]
    identifier: Vec<Identifier>,
}

/// An identifier's system and value.
#[derive(Deserialize)]
struct Identifier {
    /// The system.
    system: Option<String>,
    /// The value.
    value: Option<String>,
}

impl Page {
    /// Reads `body`, a member's answer to a search, as a page of the
    /// member whose base is `base`.
    ///
    /// # Errors
    ///
    /// The [`Unusable`] that says why the body is no page of prescriptions.
    pub fn read(body: &[u8], base: &Url) -> Result<Self, Unusable> {
        Self::read_of(body, base, RESOURCE)
    }

    /// Reads `body`, a member's answer to a search of the resource type
    /// `matched`, as a page of the member whose base is `base`; each match is
    /// kept in [`Page::prescriptions`] whatever its type is named.
    ///
    /// # Errors
    ///
    /// The [`Unusable`] that says why the body is no page of `matched`.
    pub fn read_of(body: &[u8], base: &Url, matched: &str) -> Result<Self, Unusable> {
        let head: Head = serde_json::from_slice(body).map_err(Unusable::Unreadable)?;
        if head.resource_type.as_deref() != Some("Bundle") {
            return Err(Unusable::NotBundle);
        }
        if head.kind.as_deref() != Some("searchset") {
            return Err(Unusable::NotSearchset);
        }
        let mut page = Self::default();
        for entry in head.entry {
            let resource = entry.resource.ok_or(Unusable::Empty)?;
            let read: Resource =
                serde_json::from_str(resource.get()).map_err(Unusable::Unreadable)?;
            let resource_type = read.resource_type.unwrap_or_default();
            let mode = entry.search.and_then(|search| search.mode);
            let prescribed = resource_type == matched;
            // NOTE: fhir-types 0.1.109 `codec::Value` reads through `RawValue`, so a number keeps
            // the text the member wrote, which `Value::from_serde_json` would not.
            let value: Value =
                serde_json::from_str(resource.get()).map_err(Unusable::Unreadable)?;
            let kept = Kept {
                full_url: entry.full_url,
                resource: value,
                resource_type,
            };
            match (mode.as_deref(), prescribed) {
                (Some("match") | None, true) => page.prescriptions.push(Prescription {
                    kept,
                    identifiers: read
                        .identifier
                        .into_iter()
                        .filter_map(|identifier| Some((identifier.system?, identifier.value?)))
                        .filter(|(system, value)| !system.is_empty() && !value.is_empty())
                        .collect(),
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
        let next = head
            .link
            .into_iter()
            .find(|link| link.relation.as_deref() == Some("next"))
            .and_then(|link| link.url);
        page.next = next
            .map(|next| base.join(&next))
            .transpose()
            .map_err(Unusable::Next)?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::{Page, Unusable};
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
        let page = Page::read(body, &base()).unwrap();
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
        let page = Page::read(body, &base()).unwrap();
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
        let page = Page::read(br#"{"resourceType":"Bundle","type":"searchset"}"#, &base()).unwrap();
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
            let read = Page::read(body, &base());
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
}
