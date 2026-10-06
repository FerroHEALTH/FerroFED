// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a routed read or write shows of the patient data it reached: the
//! facts [`super::record`] builds its record from.
//!
//! A request in the EHR area, the creation of an EHR, a DEMOGRAPHIC request
//! and the read of an EHR by subject each reach one node on the caller's
//! behalf (§7a.1, §12). The access is recorded when the answer names the
//! endpoint that acted for it (N31); a request refused before any node was
//! asked reached no data. The categories are read from the model ids of what
//! the access read or wrote: the `archetype_details` of the `COMPOSITION` a
//! read returns or a write sends, decoded from its canonical JSON without
//! changing a byte of the body (N22). An `EHR`, an `EHR_STATUS`, a
//! `DIRECTORY`, tags and revision history hold data of no category; any
//! other resource, a body in another format, and a deleted object the
//! access did not read are unclassified, with the reason. No specification
//! governs which bodies are read: our own design.

use std::sync::Arc;

use axum::body::Body;
use axum::response::Response;
use ehds_logging::classify::{Basis, Evidence, RootObject};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientIdentifier};
use http::{HeaderMap, Method, header};
use openehr_base::prelude::UidBasedId;
use openehr_federation::headers;
use openehr_its::json::from_canonical_json;
use openehr_its::rest::routes::{self, Lookup};
use openehr_rm::v1_2::common::change_control::original_version::OriginalVersion;
use openehr_rm::v1_2::composition::composition::Composition;
use secrecy::SecretString;

use super::{AccessLog, Accessed, Asked};
use crate::error::{self, Code};
use crate::facade::route::Arrived;
use crate::federation::Federation;

/// The operations whose resource holds data of no category.
const NO_CATEGORY: &[&str] = &[
    "ehr_create",
    "ehr_create_with_id",
    "ehr_get_by_id",
    "ehr_get_by_subject",
    "ehr_status_get_at_time",
    "ehr_status_get_by_version_id",
    "ehr_status_update",
    "ehr_status_tags_get",
    "ehr_status_tags_update",
    "ehr_status_tags_delete",
    "ehr_tags_get",
    "composition_tags_get",
    "composition_tags_update",
    "composition_tags_delete",
    "directory_create",
    "directory_update",
    "directory_delete",
    "directory_get_at_time",
    "directory_get_by_version_id",
    "versioned_composition_revision_history",
];

/// The media type of canonical JSON.
const JSON: &str = "application/json";

/// The query parameters of the read of an EHR by subject.
const SUBJECT_ID: &str = "subject_id";
/// See [`SUBJECT_ID`].
const SUBJECT_NAMESPACE: &str = "subject_namespace";

/// What one routed request shows before it is sent.
#[derive(Debug)]
pub(crate) struct Routed {
    log: Arc<AccessLog>,
    action: Action,
    operation: &'static str,
    resource: String,
    ehr_id: Option<String>,
    patient: Option<PatientIdentifier>,
    written: Option<Evidence>,
    request_id: String,
}

impl Routed {
    /// The access the request `arrived` would make, when it reaches patient
    /// data and `federation` keeps an access log.
    pub(crate) fn of(federation: &Federation, arrived: &Arrived<'_>) -> Option<Self> {
        let Lookup::Matched(matched) = routes::lookup(arrived.method, arrived.path) else {
            return None;
        };
        let (method, path, query) = (arrived.method, arrived.path, arrived.uri.query());
        let (headers, body) = (arrived.headers, arrived.body.as_ref());
        let matched = &matched;
        let log = federation.access_log()?;
        let patient_data = crate::facade::route::in_ehr_area(matched)
            || crate::facade::write::creates_ehr(matched)
            || crate::facade::route::in_demographic_area(matched)
            || crate::facade::subject::serves(matched);
        if !patient_data {
            return None;
        }
        let action = match *method {
            Method::GET | Method::HEAD => Action::Read,
            Method::POST => Action::Create,
            Method::PUT => Action::Update,
            Method::DELETE => Action::Delete,
            _ => return None,
        };
        let written = matches!(action, Action::Create | Action::Update)
            .then(|| written(matched.operation_id, headers, body));
        Some(Self {
            log: Arc::clone(log),
            action,
            operation: matched.operation_id,
            resource: path.to_owned(),
            ehr_id: matched
                .path_param("ehr_id")
                .and_then(|param| param.decoded().ok()),
            patient: crate::facade::subject::serves(matched)
                .then(|| patient(query))
                .flatten(),
            written,
            request_id: arrived.request_id.to_owned(),
        })
    }

    /// `response`, the answer to this request from `federation`, with the
    /// facts of the access attached when an endpoint acted for it.
    pub(crate) async fn attach(self, federation: &Federation, response: Response) -> Response {
        let Some(endpoint) = acting(response.headers(), headers::ENDPOINT) else {
            return response;
        };
        let system_id = acting(response.headers(), headers::SYSTEM_ID);
        let node = federation
            .snapshot()
            .endpoints()
            .find(|registered| registered.id().as_str() == endpoint)
            .map(|registered| registered.node().as_str().to_owned());
        let status = response.status();
        let (response, evidence) = match self.written {
            Some(written) => (response, written),
            None => match read(self.operation, response).await {
                Some(read) => read,
                None => return error::fixed(Code::Internal, &self.request_id),
            },
        };
        let mut response = response;
        let subject = DataSubject {
            patient: self.patient,
            ehrs: self
                .ehr_id
                .into_iter()
                .map(|ehr_id| EhrAt {
                    endpoint: endpoint.clone(),
                    ehr_id,
                })
                .collect(),
        };
        response.extensions_mut().insert(Accessed {
            log: self.log,
            action: self.action,
            operation: self.operation,
            resource: Some(self.resource),
            query: None,
            stored_query: None,
            subject,
            delivered: None,
            evidence,
            origins: vec![Asked {
                endpoint,
                node,
                system_id,
                status: status.as_str().to_owned(),
                rows: None,
                contributed: true,
            }],
        });
        response
    }
}

/// The value of the provenance header `name`, when the answer names one
/// endpoint or `system_id` (N31).
fn acting(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.contains(','))
        .map(str::to_owned)
}

/// The patient the read of an EHR by subject names, from its query string.
fn patient(query: Option<&str>) -> Option<PatientIdentifier> {
    let mut id = None;
    let mut namespace = None;
    for (name, value) in url::form_urlencoded::parse(query?.as_bytes()) {
        match name.as_ref() {
            SUBJECT_ID => id = Some(value.into_owned()),
            SUBJECT_NAMESPACE => namespace = Some(value.into_owned()),
            _ => {}
        }
    }
    Some(PatientIdentifier {
        namespace: namespace?,
        value: SecretString::from(id?),
    })
}

/// What a write of `operation` with `body` sent: the root object of a
/// `COMPOSITION` in canonical JSON.
fn written(operation: &str, headers: &HeaderMap, body: &[u8]) -> Evidence {
    if NO_CATEGORY.contains(&operation) {
        return Evidence::no_category();
    }
    if !matches!(operation, "composition_create" | "composition_update") {
        return Evidence::unreadable("operation-not-read");
    }
    if !is_json(headers) {
        return Evidence::unreadable("format-not-read");
    }
    composition(body).map_or_else(
        || Evidence::unreadable("body-not-read"),
        |object| Evidence::reached(Basis::Written, vec![object]),
    )
}

/// The answer `response` of `operation`, rebuilt, and what it returned; `None`
/// when its body cannot be read back, which an in-memory body never is.
async fn read(operation: &str, response: Response) -> Option<(Response, Evidence)> {
    if NO_CATEGORY.contains(&operation) {
        return Some((response, Evidence::no_category()));
    }
    let versioned = matches!(
        operation,
        "versioned_composition_version_get_by_id" | "versioned_composition_version_get_at_time"
    );
    if !(operation == "composition_get" || versioned) {
        return Some((response, Evidence::unreadable("operation-not-read")));
    }
    if !response.status().is_success() {
        return Some((response, Evidence::unreadable("no-object-returned")));
    }
    if !is_json(response.headers()) {
        return Some((response, Evidence::unreadable("format-not-read")));
    }
    let (parts, body) = response.into_parts();
    // NOTE: no specification governs this: our own design; a routed answer's body is the
    // node's bytes already held in memory, so reading it back takes no more than it holds.
    let bytes = axum::body::to_bytes(body, usize::MAX).await.ok()?;
    let object = if versioned {
        version(&bytes)
    } else {
        composition(&bytes)
    };
    let evidence = object.map_or_else(
        || Evidence::unreadable("body-not-read"),
        |object| Evidence::reached(Basis::Returned, vec![object]),
    );
    Some((Response::from_parts(parts, Body::from(bytes)), evidence))
}

/// Whether `headers` name canonical JSON as the body's media type.
fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case(JSON))
}

/// The root object of the `COMPOSITION` `body` is, or `None` when it is no
/// canonical `COMPOSITION`.
fn composition(body: &[u8]) -> Option<RootObject> {
    // NOTE: no specification governs this: our own design; a body that is no canonical
    // COMPOSITION has no ids to read, and the access is recorded unclassified for it.
    let composition = from_canonical_json::<Composition>(std::str::from_utf8(body).ok()?).ok()?;
    Some(root(&composition, None))
}

/// The root object of the `COMPOSITION` the `ORIGINAL_VERSION` `body` holds.
fn version(body: &[u8]) -> Option<RootObject> {
    // NOTE: no specification governs this: our own design; as for a COMPOSITION, an
    // unreadable version is recorded unclassified.
    let version =
        from_canonical_json::<OriginalVersion<Composition>>(std::str::from_utf8(body).ok()?)
            .ok()?;
    let uid = version.uid.value().to_owned();
    version.data.as_ref().map(|data| root(data, Some(uid)))
}

/// The model ids of `composition`, the version `uid` it is when known.
///
/// A `COMPOSITION` with no `archetype_details` names no id, so the access is
/// unclassified for it.
fn root(composition: &Composition, uid: Option<String>) -> RootObject {
    let details = composition.archetype_details.as_ref();
    RootObject {
        template_id: details
            .and_then(|details| details.template_id.as_ref())
            .map(|template| template.value.clone()),
        archetype_id: details.map(|details| details.archetype_id.value.clone()),
        version_uid: uid.or_else(|| match composition.uid.as_ref() {
            Some(UidBasedId::ObjectVersionId(version)) => Some(version.value().to_owned()),
            Some(_) | None => None,
        }),
    }
}
