// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version a write left at a node, read from the node's answer: the data
//! a write accessed (Regulation (EU) 2025/327 Annex II 3.2).
//!
//! ITS-REST 1.1.0 names the version a successful create, update or delete
//! left in the `ETag` header, a `version_uid` in double quotes, and the
//! `Location` of a create or an update ends with it (EHR API `ETag`,
//! `ETag_COMPOSITION`, `ETag_FOLDER`, `Location_COMPOSITION`,
//! `Location_directory`; demographic API `ETag`). The answer to an EHR create
//! names the `ehr_id`, to a `CONTRIBUTION` create the contribution's uid,
//! and to a tags write nothing, so no version is read from those.
//!
//! The `ehr_id` an EHR create left is read from the same two headers: the
//! `ETag` is the `ehr_id` in double quotes, and the `Location` is the URL
//! of the EHR resource, `{base}/ehr/{ehr_id}` (EHR API `ETag_EHR`,
//! `Location_EHR`).
//!
//! Every value is held to the `OBJECT_VERSION_ID` or the `HIER_OBJECT_ID`
//! form with `openehr-base` before it is recorded. A value that is no such
//! id is a defect of the node's answer, and the record says the id was
//! malformed and names nothing: the write was made, so its record is never
//! refused for it, and text the node put in a header never reaches the
//! record as itself, whatever it holds (§5.4, N33). No specification
//! governs that choice: our own design.

use ehds_logging::record::{CreatedEhr, WrittenVersion};
use ferrofed_engine::single_node::ehr::{from_etag, from_location};
use ferrofed_registry::id::EhrId;
use http::{HeaderMap, HeaderName, header};
use openehr_base::prelude::ObjectVersionId;

/// The last path segment but one of the `Location` of an EHR create, the
/// EHR collection (ITS-REST 1.1.0 EHR API `Location_EHR`).
const EHR_COLLECTION: &str = "ehr";

/// The write operations whose answer names no version (ITS-REST 1.1.0: the
/// `ETag` of an EHR create is the `ehr_id`, of a `CONTRIBUTION` create the
/// contribution's uid).
const NO_VERSION: &[&str] = &["ehr_create", "ehr_create_with_id", "contribution_create"];

/// The version the successful answer to the write `operation` names in
/// `headers`.
///
/// The `ETag` is read first, then the `Location`, and the first that holds
/// an `OBJECT_VERSION_ID` names the version. An answer of an operation that
/// names none, or with neither header, is [`WrittenVersion::Unnamed`]; one
/// whose headers hold no `OBJECT_VERSION_ID` is
/// [`WrittenVersion::Malformed`].
pub(crate) fn answered(operation: &str, headers: &HeaderMap) -> WrittenVersion {
    if NO_VERSION.contains(&operation) || operation.contains("_tags_") {
        return WrittenVersion::Unnamed;
    }
    let tagged = carried(headers, header::ETAG, from_etag);
    let located = carried(headers, header::LOCATION, from_location);
    match (tagged, located) {
        (None, None) => WrittenVersion::Unnamed,
        (tagged, located) => tagged
            .into_iter()
            .chain(located)
            .find(|version| matches!(version, WrittenVersion::Named(_)))
            .unwrap_or(WrittenVersion::Malformed),
    }
}

/// The version `text`, a version id a node returned, names, held to the
/// `OBJECT_VERSION_ID` form.
pub(crate) fn named(text: &str) -> WrittenVersion {
    ObjectVersionId::new(text).map_or(WrittenVersion::Malformed, |version| {
        WrittenVersion::Named(version.value().to_owned())
    })
}

/// The version the header `name` of `headers` carries, read with `read`, or
/// `None` when the answer has no such header.
fn carried(
    headers: &HeaderMap,
    name: HeaderName,
    read: fn(&str) -> Option<String>,
) -> Option<WrittenVersion> {
    let value = headers.get(name)?;
    Some(
        value
            .to_str()
            .ok()
            .and_then(read)
            .map_or(WrittenVersion::Malformed, |text| named(&text)),
    )
}

/// The `ehr_id` the successful answer to an EHR create names in `headers`,
/// held to the `HIER_OBJECT_ID` form.
///
/// The `ETag` is read first, then the `Location`, and the first that holds
/// an `ehr_id` names it. An answer with neither header is
/// [`CreatedEhr::Unnamed`]; one whose headers hold no `ehr_id`, or two that
/// differ, is [`CreatedEhr::Malformed`].
pub(crate) fn created(headers: &HeaderMap) -> CreatedEhr {
    let tagged = held(headers, header::ETAG, from_etag);
    let located = held(headers, header::LOCATION, in_collection);
    match (tagged, located) {
        (Held::Absent, Held::Absent) => CreatedEhr::Unnamed,
        (Held::Named(tagged), Held::Named(located)) if tagged != located => CreatedEhr::Malformed,
        (Held::Named(ehr_id), _) | (_, Held::Named(ehr_id)) => {
            CreatedEhr::Named(ehr_id.as_str().to_owned())
        }
        _ => CreatedEhr::Malformed,
    }
}

/// What one header of an EHR create's answer holds.
enum Held {
    /// The answer has no such header.
    Absent,
    /// The header names this `ehr_id`.
    Named(EhrId),
    /// The header holds no `HIER_OBJECT_ID`.
    Malformed,
}

/// What the header `name` of `headers` holds, read with `read`.
fn held(headers: &HeaderMap, name: HeaderName, read: fn(&str) -> Option<String>) -> Held {
    let Some(value) = headers.get(name) else {
        return Held::Absent;
    };
    // NOTE: no specification governs this: our own design; a header that names no
    // HIER_OBJECT_ID is a defect of the answer, recorded as malformed.
    value
        .to_str()
        .ok()
        .and_then(read)
        .and_then(|text| EhrId::new(text).ok())
        .map_or(Held::Malformed, Held::Named)
}

/// The `ehr_id` a `Location` names when it is the URL of an EHR resource,
/// `{base}/ehr/{ehr_id}` (ITS-REST 1.1.0 EHR API `Location_EHR`).
fn in_collection(location: &str) -> Option<String> {
    let url = url::Url::parse(location.trim()).ok()?;
    let mut segments = url.path_segments()?.filter(|segment| !segment.is_empty());
    let parent = segments.nth_back(1)?;
    (parent == EHR_COLLECTION)
        .then(|| from_location(location))
        .flatten()
}

#[cfg(test)]
mod tests {
    use ehds_logging::record::{CreatedEhr, WrittenVersion};
    use http::{HeaderMap, HeaderValue, header};

    use super::{answered, created};

    const UID: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::2";

    const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

    #[test]
    fn an_ehr_create_names_its_ehr_id_in_the_etag_or_the_location() {
        let location = format!("https://cdr.example.org/v1/ehr/{EHR}");
        let named = CreatedEhr::Named(EHR.to_owned());
        for answer in [
            headers(&[(header::ETAG, &format!("\"{EHR}\""))]),
            headers(&[(header::ETAG, &format!("W/\"{EHR}\""))]),
            headers(&[(header::LOCATION, &location)]),
            headers(&[
                (header::ETAG, &format!("\"{EHR}\"")),
                (header::LOCATION, &location),
            ]),
            headers(&[(header::ETAG, "unquoted"), (header::LOCATION, &location)]),
        ] {
            assert_eq!(named, created(&answer), "{answer:?}");
        }
    }

    #[test]
    fn an_ehr_create_answer_with_no_header_names_no_ehr_id() {
        assert_eq!(CreatedEhr::Unnamed, created(&HeaderMap::new()));
    }

    #[test]
    fn an_ehr_create_answer_that_holds_no_ehr_id_or_two_is_malformed() {
        let other = "0e2c2f4b-5b0a-4f4e-9d1a-2f7d1c0b9a11";
        for answer in [
            headers(&[(header::ETAG, "\"\"")]),
            headers(&[(header::ETAG, "\"urn:oid:2.999.1|38kq\"")]),
            headers(&[(
                header::LOCATION,
                "https://cdr.example.org/v1/ehr?subject_id=38kq",
            )]),
            headers(&[(
                header::LOCATION,
                &format!("https://cdr.example.org/v1/party/{EHR}"),
            )]),
            headers(&[
                (header::ETAG, &format!("\"{EHR}\"")),
                (
                    header::LOCATION,
                    &format!("https://cdr.example.org/v1/ehr/{other}"),
                ),
            ]),
        ] {
            assert_eq!(CreatedEhr::Malformed, created(&answer), "{answer:?}");
        }
    }

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                name.clone(),
                HeaderValue::from_str(value).expect("a header value"),
            );
        }
        map
    }

    #[test]
    fn the_etag_names_the_version_weak_or_strong() {
        for tag in [format!("\"{UID}\""), format!("W/\"{UID}\"")] {
            assert_eq!(
                WrittenVersion::Named(UID.to_owned()),
                answered("composition_update", &headers(&[(header::ETAG, &tag)]))
            );
        }
    }

    #[test]
    fn the_location_names_it_when_the_etag_does_not() {
        let location = format!("https://cdr.example.org/v1/ehr/7d44b88c/composition/{UID}");
        assert_eq!(
            WrittenVersion::Named(UID.to_owned()),
            answered(
                "composition_create",
                &headers(&[(header::ETAG, "\"opaque\""), (header::LOCATION, &location)])
            )
        );
        assert_eq!(
            WrittenVersion::Named(UID.to_owned()),
            answered(
                "directory_create",
                &headers(&[(header::LOCATION, &location)])
            )
        );
    }

    #[test]
    fn an_answer_with_no_version_header_names_none() {
        assert_eq!(
            WrittenVersion::Unnamed,
            answered("directory_delete", &HeaderMap::new())
        );
    }

    #[test]
    fn a_header_that_holds_no_version_id_is_malformed() {
        for tag in ["\"opaque\"", "unquoted", "\"\""] {
            assert_eq!(
                WrittenVersion::Malformed,
                answered("composition_create", &headers(&[(header::ETAG, tag)]))
            );
        }
    }

    #[test]
    fn an_ehr_create_a_contribution_and_a_tags_write_name_no_version() {
        let tagged = headers(&[(header::ETAG, &format!("\"{UID}\""))]);
        for operation in [
            "ehr_create",
            "ehr_create_with_id",
            "contribution_create",
            "composition_tags_update",
            "ehr_status_tags_delete",
        ] {
            assert_eq!(WrittenVersion::Unnamed, answered(operation, &tagged));
        }
    }
}
