// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The id the FHIR face names a member's prescription by, which locates it
//! again with no state at the gateway.
//!
//! The id names the member's own endpoint, the member's `ehr_id` for the
//! patient and the member's own logical id of the resource. A prescription the
//! face maps from a member's composition (the map mode) names the member's
//! Query API endpoint, its `ehr_id`, the UUID of the composition's versioned
//! object and the prescription's position among those the composition maps
//! to. It is a FHIR R4
//! `id`, 1 to 64 of `[A-Za-z0-9\-\.]` (FHIR R4 Datatypes `id`), and an
//! `OBJECT_VERSION_ID` or a URL is none, so the three are packed into at most
//! 48 bytes written in the 64 characters an `id` admits: the base64url
//! alphabet of RFC 4648 §5 with `.` in place of `_`, unpadded. The endpoint
//! is named by the first 6 bytes of the SHA-256 of its `endpoint_id`, and an
//! `ehr_id` or a resource id that is a UUID in its lowercase hyphenated form
//! by its 16 bytes; anything else travels as its text. A prescription whose
//! parts do not fit has no such id.
//!
//! The id is a locator alone: it carries no patient identifier (§5.4.1,
//! N33), and every read is still held to the caller's grant and asked of the
//! member by the patient's `ehr_id` there (N26). The form is our own design:
//! no specification governs it.

use aws_lc_rs::digest::{SHA256, digest};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_registry::id::{EhrId, EndpointId};
use uuid::Uuid;

use crate::face_path::{LENGTH, is_fhir_id};

/// The most bytes an id packs: [`LENGTH`] characters of 6 bits each.
const PACKED: usize = 48;

/// The first byte of the form this module writes; its low bits say which
/// part is a UUID.
const FORM: u8 = 0x10;

/// The bit of the first byte that says the `ehr_id` is a UUID.
const EHR_UUID: u8 = 0x01;

/// The bit of the first byte that says the resource id is a UUID.
const RESOURCE_UUID: u8 = 0x02;

/// The bit of the first byte that says the resource is one prescription of a
/// member's composition, mapped by the face: the resource id is the
/// composition's UUID and one byte after it names the prescription.
const MAPPED: u8 = 0x04;

/// The bits of the first byte the form defines.
const FLAGS: u8 = EHR_UUID | RESOURCE_UUID | MAPPED;

/// The bytes of the SHA-256 of an `endpoint_id` that name the endpoint.
const TAG: usize = 6;

/// The bytes of a UUID.
const UUID: usize = 16;

/// One member's prescription: its own endpoint, its `ehr_id` for the
/// patient and its own logical id of the resource.
///
/// `Debug` shows the endpoint alone.
#[derive(Clone, PartialEq, Eq)]
pub struct Locator {
    endpoint: EndpointId,
    ehr_id: EhrId,
    resource: String,
    entry: Option<u8>,
}

impl std::fmt::Debug for Locator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locator")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// Why an id locates no one prescription; no variant's text names a value
/// the id carries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Unlocated {
    /// The id is not one this module writes.
    #[error("the id is no prescription id the face writes")]
    Malformed,
    /// The id names no endpoint among those it is read against.
    #[error("the id names no member endpoint that serves prescriptions")]
    Unknown,
    /// The id names more than one of the endpoints it is read against.
    #[error("the id names more than one member endpoint")]
    Ambiguous(Vec<EndpointId>),
}

impl Locator {
    /// The prescription `resource`, the member's own logical id, held under
    /// `ehr_id` at the member's own endpoint `endpoint`.
    #[must_use]
    pub fn new(endpoint: EndpointId, ehr_id: EhrId, resource: impl Into<String>) -> Self {
        Self {
            endpoint,
            ehr_id,
            resource: resource.into(),
            entry: None,
        }
    }

    /// The prescription at position `entry` among those the face maps from the
    /// composition of the versioned object `composition`, held under `ehr_id`
    /// at the member whose Query API endpoint is `endpoint`.
    #[must_use]
    pub fn mapped(endpoint: EndpointId, ehr_id: EhrId, composition: Uuid, entry: u8) -> Self {
        Self {
            endpoint,
            ehr_id,
            resource: composition.hyphenated().to_string(),
            entry: Some(entry),
        }
    }

    /// Returns the position of the prescription among those the face maps
    /// from the composition [`Locator::resource`] names, when the id names a
    /// mapped prescription.
    #[must_use]
    pub fn entry(&self) -> Option<u8> {
        self.entry
    }

    /// Returns the member's own endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointId {
        &self.endpoint
    }

    /// Returns the member's `ehr_id` for the patient.
    #[must_use]
    pub fn ehr_id(&self) -> &EhrId {
        &self.ehr_id
    }

    /// Returns the member's own logical id of the resource.
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the id, or `None` when the resource id is no FHIR R4 `id` or
    /// the parts do not fit in [`LENGTH`] characters.
    #[must_use]
    pub fn id(&self) -> Option<String> {
        if !is_fhir_id(&self.resource) {
            return None;
        }
        let ehr_uuid = canonical_uuid(self.ehr_id.as_str());
        let resource_uuid = canonical_uuid(&self.resource);
        let mut packed = Vec::with_capacity(PACKED);
        let mut form = FORM;
        if ehr_uuid.is_some() {
            form |= EHR_UUID;
        }
        if self.entry.is_some() {
            resource_uuid?;
            form |= MAPPED;
        }
        if resource_uuid.is_some() {
            form |= RESOURCE_UUID;
        }
        packed.push(form);
        packed.extend_from_slice(&tag(&self.endpoint));
        if let Some(uuid) = ehr_uuid {
            packed.extend_from_slice(uuid.as_bytes());
        } else {
            let text = self.ehr_id.as_str().as_bytes();
            packed.push(u8::try_from(text.len()).ok()?);
            packed.extend_from_slice(text);
        }
        match resource_uuid {
            Some(uuid) => packed.extend_from_slice(uuid.as_bytes()),
            None => packed.extend_from_slice(self.resource.as_bytes()),
        }
        if let Some(entry) = self.entry {
            packed.push(entry);
        }
        if packed.len() > PACKED {
            return None;
        }
        Some(URL_SAFE_NO_PAD.encode(packed).replace('_', "."))
    }

    /// Reads `id` as a prescription of one of `endpoints`, the member
    /// endpoints the id may name.
    ///
    /// Only the one text [`Locator::id`] writes for a prescription is read
    /// as it, so no prescription has two ids.
    ///
    /// # Errors
    ///
    /// [`Unlocated::Malformed`] for an id this module does not write,
    /// [`Unlocated::Unknown`] when it names none of `endpoints`, and
    /// [`Unlocated::Ambiguous`] when it names several.
    pub fn read<'e>(
        id: &str,
        endpoints: impl IntoIterator<Item = &'e EndpointId>,
    ) -> Result<Self, Unlocated> {
        if id.len() > LENGTH || id.contains('_') {
            return Err(Unlocated::Malformed);
        }
        let packed = URL_SAFE_NO_PAD
            .decode(id.replace('.', "_"))
            .map_err(|_undecoded| Unlocated::Malformed)?;
        let (&form, rest) = packed.split_first().ok_or(Unlocated::Malformed)?;
        if form & !FLAGS != FORM {
            return Err(Unlocated::Malformed);
        }
        let (named, rest) = rest.split_at_checked(TAG).ok_or(Unlocated::Malformed)?;
        let (ehr_id, rest) = if form & EHR_UUID == 0 {
            let (&length, rest) = rest.split_first().ok_or(Unlocated::Malformed)?;
            let (text, rest) = rest
                .split_at_checked(usize::from(length))
                .ok_or(Unlocated::Malformed)?;
            (text_of(text)?.to_owned(), rest)
        } else {
            let (bytes, rest) = rest.split_at_checked(UUID).ok_or(Unlocated::Malformed)?;
            (uuid_of(bytes)?, rest)
        };
        let (rest, entry) = if form & MAPPED == 0 {
            (rest, None)
        } else {
            let (&entry, rest) = rest.split_last().ok_or(Unlocated::Malformed)?;
            (rest, Some(entry))
        };
        let resource = if form & RESOURCE_UUID == 0 {
            text_of(rest)?.to_owned()
        } else {
            if rest.len() != UUID {
                return Err(Unlocated::Malformed);
            }
            uuid_of(rest)?
        };
        let ehr_id = EhrId::new(ehr_id).map_err(|_unparsed| Unlocated::Malformed)?;
        let mut matched: Vec<EndpointId> = endpoints
            .into_iter()
            .filter(|endpoint| tag(endpoint).as_slice() == named)
            .cloned()
            .collect();
        matched.sort();
        matched.dedup();
        let endpoint = match matched.len() {
            0 => return Err(Unlocated::Unknown),
            1 => matched.pop().ok_or(Unlocated::Unknown)?,
            _ => return Err(Unlocated::Ambiguous(matched)),
        };
        let located = Self {
            endpoint,
            ehr_id,
            resource,
            entry,
        };
        // NOTE: no specification governs this: our own design; a text that packs the same
        // parts another way would give one prescription two ids, so only the written one is read.
        if located.id().as_deref() != Some(id) {
            return Err(Unlocated::Malformed);
        }
        Ok(located)
    }
}

/// The bytes that name `endpoint`: the first [`TAG`] of the SHA-256 of its
/// `endpoint_id`.
fn tag(endpoint: &EndpointId) -> [u8; TAG] {
    let hashed = digest(&SHA256, endpoint.as_str().as_bytes());
    let mut tag = [0; TAG];
    for (byte, hashed) in tag.iter_mut().zip(hashed.as_ref()) {
        *byte = *hashed;
    }
    tag
}

/// The UUID `text` is, when it is one in its lowercase hyphenated form,
/// the one form that packs to its bytes and back unchanged.
fn canonical_uuid(text: &str) -> Option<Uuid> {
    Uuid::parse_str(text)
        .ok()
        .filter(|uuid| uuid.hyphenated().to_string() == text)
}

/// The lowercase hyphenated text of the UUID `bytes`.
fn uuid_of(bytes: &[u8]) -> Result<String, Unlocated> {
    Uuid::from_slice(bytes)
        .map(|uuid| uuid.hyphenated().to_string())
        .map_err(|_unsized| Unlocated::Malformed)
}

/// The text `bytes` are, when they are UTF-8.
fn text_of(bytes: &[u8]) -> Result<&str, Unlocated> {
    std::str::from_utf8(bytes).map_err(|_unread| Unlocated::Malformed)
}

#[cfg(test)]
mod tests {
    use super::{LENGTH, Locator, Unlocated, is_fhir_id};
    use ferrofed_registry::id::{EhrId, EndpointId};

    const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

    fn endpoint(id: &str) -> EndpointId {
        EndpointId::new(id).unwrap()
    }

    fn locator(ehr_id: &str, resource: &str) -> Locator {
        Locator::new(
            endpoint("node-a-mpd"),
            EhrId::new(ehr_id).unwrap(),
            resource,
        )
    }

    #[test]
    fn an_id_is_a_fhir_id_and_reads_back_to_its_parts() {
        let endpoints = [endpoint("node-b-mpd"), endpoint("node-a-mpd")];
        for (ehr_id, resource) in [
            (EHR, "0b5e0a3e-6f1a-4c3e-9f0e-2d6b1e7c9a10"),
            (EHR, "rx-a1"),
            ("12345", "0b5e0a3e-6f1a-4c3e-9f0e-2d6b1e7c9a10"),
            ("12345", "rx.A-1"),
            ("7D44B88C-4199-4BAD-97DC-D78268E01398", "rx-1"),
        ] {
            let located = locator(ehr_id, resource);
            let id = located.id().unwrap();
            assert!(is_fhir_id(&id), "{id}");
            let read = Locator::read(&id, &endpoints).unwrap();
            assert_eq!(read, located, "{ehr_id} {resource}");
            assert_eq!(read.ehr_id().as_str(), ehr_id);
            assert_eq!(read.resource(), resource);
        }
    }

    #[test]
    fn the_id_is_the_same_each_time() {
        assert_eq!(locator(EHR, "rx-a1").id(), locator(EHR, "rx-a1").id());
        assert_ne!(locator(EHR, "rx-a1").id(), locator(EHR, "rx-a2").id());
    }

    #[test]
    fn parts_that_do_not_fit_have_no_id() {
        assert_eq!(None, locator(EHR, &"r".repeat(LENGTH)).id());
        assert_eq!(None, locator(EHR, "rx/1").id(), "no FHIR id");
    }

    #[test]
    fn an_id_naming_another_endpoint_is_unknown() {
        let id = locator(EHR, "rx-a1").id().unwrap();
        assert_eq!(
            Err(Unlocated::Unknown),
            Locator::read(&id, &[endpoint("node-b-mpd")])
        );
    }

    #[test]
    fn an_endpoint_listed_twice_is_one_endpoint() {
        let id = locator(EHR, "rx-a1").id().unwrap();
        let named = Locator::read(&id, &[endpoint("node-a-mpd"), endpoint("node-a-mpd")]);
        assert_eq!(
            Ok(locator(EHR, "rx-a1")),
            named,
            "one endpoint listed twice"
        );
    }

    #[test]
    fn a_text_the_face_never_wrote_is_malformed() {
        let endpoints = [endpoint("node-a-mpd")];
        let written = locator(EHR, "rx-a1").id().unwrap();
        let tampered = format!("A{}", written.get(1..).unwrap());
        for id in [
            "synthetic-rx-1",
            "",
            "AAAA",
            tampered.as_str(),
            "A_AA",
            &"A".repeat(LENGTH + 4),
        ] {
            assert_eq!(
                Err(Unlocated::Malformed),
                Locator::read(id, &endpoints),
                "{id}"
            );
        }
    }

    #[test]
    fn a_mapped_prescription_reads_back_to_its_composition_and_position() {
        let endpoints = [endpoint("node-a-query"), endpoint("node-b-query")];
        let composition = uuid::Uuid::parse_str("5f0c2a9e-0000-4000-8000-000000000001").unwrap();
        for (ehr_id, entry) in [(EHR, 0), (EHR, 3), ("12345", 255)] {
            let located = Locator::mapped(
                endpoint("node-a-query"),
                EhrId::new(ehr_id).unwrap(),
                composition,
                entry,
            );
            let id = located.id().unwrap();
            assert!(is_fhir_id(&id), "{id}");
            let read = Locator::read(&id, &endpoints).unwrap();
            assert_eq!(read, located);
            assert_eq!(Some(entry), read.entry());
            assert_eq!("5f0c2a9e-0000-4000-8000-000000000001", read.resource());
        }
    }

    #[test]
    fn a_mapped_prescription_and_a_native_one_never_share_an_id() {
        let composition = uuid::Uuid::parse_str("5f0c2a9e-0000-4000-8000-000000000001").unwrap();
        let ehr_id = EhrId::new(EHR).unwrap();
        let mapped = Locator::mapped(endpoint("node-a-mpd"), ehr_id.clone(), composition, 0);
        let native = Locator::new(
            endpoint("node-a-mpd"),
            ehr_id,
            "5f0c2a9e-0000-4000-8000-000000000001",
        );
        assert_ne!(mapped.id(), native.id());
        assert_eq!(
            None,
            Locator::read(&native.id().unwrap(), &[endpoint("node-a-mpd")])
                .unwrap()
                .entry()
        );
    }

    #[test]
    fn no_debug_shows_the_ehr_id_or_the_resource() {
        let shown = format!("{:?}", locator(EHR, "rx-secret-9"));
        assert!(
            !shown.contains("7d44") && !shown.contains("secret"),
            "{shown}"
        );
    }
}
