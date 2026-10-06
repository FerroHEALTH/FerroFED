// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one resolver of a reference inside a received document.
//!
//! FHIR R4 resolves a reference inside a `Bundle` against the entries'
//! `fullUrl`s: a fragment `#id` names a resource contained in the resource
//! that holds it; an absolute reference names the entry whose `fullUrl`
//! equals it; a relative `Type/id` is made absolute against the base of the
//! holding entry's REST-style `fullUrl` first; and a version-specific
//! reference is matched without its `_history` part, then on
//! `meta.versionId` (<https://hl7.org/fhir/R4/bundle.html#references>,
//! <https://hl7.org/fhir/R4/references.html#literal>).
//!
//! Every rule of the receive path that follows a reference calls
//! [`resolve`]: the subject of the document, every reference of every entry,
//! and whatever later reads the document. It answers exactly one entry, or a
//! typed [`ReferenceError`]; it never compares text a second way.
//!
//! A reference and a `fullUrl` are read in one canonical spelling only: the
//! R4 literal reference grammar, `http` and `https` with a lower-case host,
//! `urn:uuid:` with a lower-case UUID and `urn:oid:` with an OID. Whitespace,
//! a query string, a fragment, percent-encoding, a trailing or doubled
//! slash, a dot segment, a resource type in another case and any other
//! scheme are refused, because a lenient reader could take such a spelling
//! for an entry this resolver would not, and two readers must never resolve
//! one document's references to different entries.

use fhir_types::codec::EncodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::schema::SCHEMAS;

/// The longest resource id and version id R4 admits
/// (<https://hl7.org/fhir/R4/datatypes.html#id>).
const ID_LENGTH: usize = 64;

/// What a reference resolves to: always exactly one entry of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The resource of the entry at this index.
    Entry(usize),
    /// A resource contained in the resource of the entry at `entry`.
    Contained {
        /// The index of the entry whose resource contains it.
        entry: usize,
        /// The contained resource's id.
        id: String,
    },
}

/// Why a reference resolves to no single entry of the document.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReferenceError {
    /// The reference carries no `reference`: it names its target by
    /// `identifier` or `display` alone, or not at all.
    #[error("the reference names no target by reference")]
    Unreferenced {
        /// Whether it carries an `identifier`.
        identifier: bool,
    },
    /// The `reference` is not spelled in the one canonical form.
    #[error("the reference is not spelled in its canonical form")]
    NonCanonical,
    /// A `#id` names no resource contained in the holding resource.
    #[error("the reference names no contained resource")]
    LocalMissing,
    /// A relative reference is held by an entry whose `fullUrl` is no
    /// REST-style URL to resolve it against.
    #[error("a relative reference is held by an entry with no REST-style fullUrl")]
    RelativeWithoutBase,
    /// The reference names no entry of the document.
    #[error("the reference names no entry of the document")]
    NotInBundle {
        /// The resource type its path names, when it names one.
        kind: Option<String>,
    },
    /// The reference names more than one entry.
    #[error("the reference names more than one entry")]
    Ambiguous,
    /// A version-specific reference names an entry of no or another
    /// `meta.versionId`.
    #[error("the reference names a version its entry does not carry")]
    VersionMismatch,
    /// The reference's `type` is not the type of its target.
    #[error("the reference's type is not its target's type")]
    TypeDisagrees,
    /// The reference's `identifier` is not one of its target's.
    #[error("the reference's identifier is not one of its target's")]
    IdentifierDisagrees,
    /// The document or the reference cannot be encoded as JSON.
    #[error("the document cannot be encoded as JSON")]
    Unencodable {
        /// The encode failure.
        #[source]
        source: EncodeError,
    },
}

/// One entry of the encoded document: its `fullUrl` and its resource.
#[derive(Debug, Clone, Copy)]
pub(super) struct Entry<'d> {
    pub(super) full_url: Option<&'d str>,
    pub(super) resource: Option<&'d Object>,
}

impl Entry<'_> {
    /// Returns the type of the entry's resource.
    pub(super) fn resource_type(&self) -> Option<&str> {
        self.resource?.get("resourceType")?.as_str()
    }
}

/// Returns the entries of `tree`, the encoded document, in order.
pub(super) fn entries(tree: &Object) -> Vec<Entry<'_>> {
    tree.get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .map(|entry| Entry {
            full_url: entry.get("fullUrl").and_then(Value::as_str),
            resource: entry.get("resource").and_then(Value::as_object),
        })
        .collect()
}

/// A URL in its canonical spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Url<'u> {
    /// `urn:uuid:` or `urn:oid:`.
    Urn,
    /// An `http` or `https` URL whose path ends with `Type/id`, with its
    /// version when it is version specific.
    Rest {
        /// The URL up to the `/Type/id` part.
        base: &'u str,
        /// The resource type.
        kind: &'u str,
        /// The resource id.
        id: &'u str,
        /// The version, from a `/_history/` part.
        version: Option<&'u str>,
    },
    /// Any other `http` or `https` URL.
    Other,
}

impl<'u> Url<'u> {
    /// Reads `text` as an absolute URL in its canonical spelling, or `None`
    /// when it is not one.
    pub(super) fn parse(text: &'u str) -> Option<Self> {
        if let Some(uuid) = text.strip_prefix("urn:uuid:") {
            return canonical_uuid(uuid).then_some(Self::Urn);
        }
        if let Some(oid) = text.strip_prefix("urn:oid:") {
            return canonical_oid(oid).then_some(Self::Urn);
        }
        let rest = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"))?;
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let canonical_host = !host.is_empty()
            && host.chars().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, '-' | '.' | ':')
            });
        let segments: Vec<&str> = path.split('/').collect();
        let canonical_path = path.is_empty()
            || segments.iter().all(|segment| {
                !segment.is_empty()
                    && *segment != "."
                    && *segment != ".."
                    && segment.chars().all(|character| {
                        character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '.' | '_' | '~')
                    })
            });
        if !canonical_host || !canonical_path {
            return None;
        }
        let (unversioned, version) = match text.rsplit_once("/_history/") {
            Some((unversioned, version)) => (unversioned, Some(version)),
            None => (text, None),
        };
        if version.is_some_and(|version| !canonical_id(version)) {
            return None;
        }
        let rest_style = unversioned.rsplit_once('/').and_then(|(head, id)| {
            let (base, kind) = head.rsplit_once('/')?;
            (SCHEMAS.is_resource(kind) && canonical_id(id) && base.contains("://")).then_some(
                Self::Rest {
                    base,
                    kind,
                    id,
                    version,
                },
            )
        });
        match (rest_style, version) {
            (Some(url), _) => Some(url),
            (None, Some(_)) => None,
            (None, None) => Some(Self::Other),
        }
    }
}

/// Returns whether `text` is an R4 `id`: 1 to 64 of `A-Z`, `a-z`, `0-9`,
/// `-` and `.`.
fn canonical_id(text: &str) -> bool {
    (1..=ID_LENGTH).contains(&text.len())
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.'))
}

/// Returns whether `text` is a UUID in lower-case hexadecimal, 8-4-4-4-12
/// (<https://www.rfc-editor.org/rfc/rfc9562#section-4>).
fn canonical_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12])
        && groups.iter().all(|group| {
            group
                .chars()
                .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
        })
}

/// Returns whether `text` is an OID, `0`, `1` or `2` and further arcs
/// without leading zeros (<https://hl7.org/fhir/R4/datatypes.html#oid>).
fn canonical_oid(text: &str) -> bool {
    let mut arcs = text.split('.');
    let first = arcs.next();
    let rest: Vec<&str> = arcs.collect();
    matches!(first, Some("0" | "1" | "2"))
        && !rest.is_empty()
        && rest.iter().all(|arc| {
            !arc.is_empty()
                && arc.chars().all(|character| character.is_ascii_digit())
                && (*arc == "0" || !arc.starts_with('0'))
        })
}

/// Resolves `reference`, held by the resource of the entry at `holder`, in
/// `bundle`.
///
/// # Errors
///
/// Returns the [`ReferenceError`] that keeps the reference from naming
/// exactly one entry, or [`ReferenceError::Unencodable`] when the document
/// or the reference does not encode as JSON.
pub fn resolve(
    bundle: &Bundle,
    holder: usize,
    reference: &Reference,
) -> Result<Target, ReferenceError> {
    let tree = Json::to_json(bundle).map_err(|source| ReferenceError::Unencodable { source })?;
    let reference =
        Json::to_json(reference).map_err(|source| ReferenceError::Unencodable { source })?;
    resolve_in(&entries(&tree), holder, &reference)
}

/// Resolves the encoded `reference`, held by the resource of the entry at
/// `holder`, among the encoded `entries`.
pub(super) fn resolve_in(
    entries: &[Entry<'_>],
    holder: usize,
    reference: &Object,
) -> Result<Target, ReferenceError> {
    let declared = reference.get("type").and_then(Value::as_str);
    let identifier = reference.get("identifier").and_then(Value::as_object);
    let Some(text) = reference.get("reference").and_then(Value::as_str) else {
        return Err(ReferenceError::Unreferenced {
            identifier: identifier.is_some(),
        });
    };
    if declared.is_some_and(|kind| !SCHEMAS.is_resource(kind)) {
        return Err(ReferenceError::TypeDisagrees);
    }
    let (target, resource) = if let Some(id) = text.strip_prefix('#') {
        local(entries, holder, id)?
    } else {
        let index = entry(entries, holder, text)?;
        let resource = entries
            .get(index)
            .and_then(|entry| entry.resource)
            .ok_or(ReferenceError::NotInBundle { kind: None })?;
        (Target::Entry(index), resource)
    };
    let kind = resource.get("resourceType").and_then(Value::as_str);
    if declared.is_some_and(|declared| Some(declared) != kind) {
        return Err(ReferenceError::TypeDisagrees);
    }
    if let Some(identifier) = identifier
        && !carries(resource, identifier)
    {
        return Err(ReferenceError::IdentifierDisagrees);
    }
    Ok(target)
}

/// Resolves a `#id`, or `#` alone for the holding resource itself
/// (<https://hl7.org/fhir/R4/references.html#contained>).
fn local<'d>(
    entries: &[Entry<'d>],
    holder: usize,
    id: &str,
) -> Result<(Target, &'d Object), ReferenceError> {
    let resource = entries
        .get(holder)
        .and_then(|entry| entry.resource)
        .ok_or(ReferenceError::LocalMissing)?;
    if id.is_empty() {
        return Ok((Target::Entry(holder), resource));
    }
    if !canonical_id(id) {
        return Err(ReferenceError::NonCanonical);
    }
    let mut named = resource
        .get("contained")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_object)
        .filter(|contained| contained.get("id").and_then(Value::as_str) == Some(id));
    match (named.next(), named.next()) {
        (Some(contained), None) => Ok((
            Target::Contained {
                entry: holder,
                id: id.to_owned(),
            },
            contained,
        )),
        (Some(_), Some(_)) => Err(ReferenceError::Ambiguous),
        (None, _) => Err(ReferenceError::LocalMissing),
    }
}

/// Resolves a relative or absolute reference to the index of one entry.
fn entry(entries: &[Entry<'_>], holder: usize, text: &str) -> Result<usize, ReferenceError> {
    let absolute = if text.contains(':') {
        Url::parse(text).ok_or(ReferenceError::NonCanonical)?;
        text.to_owned()
    } else {
        let segments: Vec<&str> = text.split('/').collect();
        let relative = match segments.as_slice() {
            [kind, id] => SCHEMAS.is_resource(kind) && canonical_id(id),
            [kind, id, "_history", version] => {
                SCHEMAS.is_resource(kind) && canonical_id(id) && canonical_id(version)
            }
            _ => false,
        };
        if !relative {
            return Err(ReferenceError::NonCanonical);
        }
        let from = entries
            .get(holder)
            .and_then(|entry| entry.full_url)
            .and_then(Url::parse);
        let Some(Url::Rest { base, .. }) = from else {
            return Err(ReferenceError::RelativeWithoutBase);
        };
        format!("{base}/{text}")
    };
    let (unversioned, version, kind) = match Url::parse(&absolute) {
        Some(Url::Rest {
            base,
            kind,
            id,
            version,
        }) => (format!("{base}/{kind}/{id}"), version, Some(kind)),
        Some(Url::Urn | Url::Other) => (absolute.clone(), None, None),
        None => return Err(ReferenceError::NonCanonical),
    };
    let named: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.full_url == Some(unversioned.as_str()))
        .map(|(index, _)| index)
        .collect();
    let named: Vec<usize> = match version {
        None => named,
        Some(version) => {
            let versioned: Vec<usize> = named
                .iter()
                .copied()
                .filter(|index| version_of(entries, *index) == Some(version))
                .collect();
            if versioned.is_empty() && !named.is_empty() {
                return Err(ReferenceError::VersionMismatch);
            }
            versioned
        }
    };
    match named.as_slice() {
        [index] => Ok(*index),
        [] => Err(ReferenceError::NotInBundle {
            kind: kind.map(str::to_owned),
        }),
        _ => Err(ReferenceError::Ambiguous),
    }
}

/// Returns the `meta.versionId` of the entry at `index`.
fn version_of<'d>(entries: &[Entry<'d>], index: usize) -> Option<&'d str> {
    entries
        .get(index)?
        .resource?
        .get("meta")?
        .get("versionId")?
        .as_str()
}

/// Returns whether `resource` carries `identifier`: an identifier of the
/// same system and value.
// NOTE: R4 asks a reference's identifier to agree with its target
// (<https://hl7.org/fhir/R4/references.html#logical>); an identifier the target does
// not carry cannot be shown to agree, so it is refused (our own reading).
fn carries(resource: &Object, identifier: &Object) -> bool {
    let wanted = (
        identifier.get("system").and_then(Value::as_str),
        identifier.get("value").and_then(Value::as_str),
    );
    if wanted.1.is_none() {
        return false;
    }
    let carried: Vec<&Value> = match resource.get("identifier") {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    carried.iter().any(|carried| {
        (
            carried.get("system").and_then(Value::as_str),
            carried.get("value").and_then(Value::as_str),
        ) == wanted
    })
}
