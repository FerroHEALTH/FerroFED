// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which area of the ITS-REST surface a matched route is in (§7a.1).
//!
//! The areas are the EHR area under a path `ehr_id`, the creation of an
//! EHR, the read of an EHR by subject and the DEMOGRAPHIC area, which the
//! facade routes and the access log records. No specification governs the layout: our own design.

use ferrofed_registry::id::EhrId;
use openehr_its::rest::routes::RouteMatch;

/// The API group of the EHR area (§7a.1).
pub const EHR_GROUP: &str = "ehr";

/// The API group of the DEMOGRAPHIC area (§7a.1).
pub const DEMOGRAPHIC_GROUP: &str = "demographic";

/// The path template every single-node EHR resource sits at or below
/// (§7a.1).
const EHR_RESOURCE: &str = "/ehr/{ehr_id}";

/// The path parameter that names the EHR (§12.5).
const EHR_ID_PARAM: &str = "ehr_id";

/// The ITS-REST operation that reads an EHR by its subject.
const BY_SUBJECT: &str = "ehr_get_by_subject";

/// Whether `matched` is an operation of the DEMOGRAPHIC area under
/// `{base}/v1/demographic/` (§7a.1, N32).
#[must_use]
pub fn in_demographic_area(matched: &RouteMatch) -> bool {
    matched.group == DEMOGRAPHIC_GROUP
}

/// Whether `matched` is an operation on an EHR resource addressed by a path
/// `ehr_id` (§7a.1).
#[must_use]
pub fn in_ehr_area(matched: &RouteMatch) -> bool {
    matched.group == EHR_GROUP
        && matched
            .template
            .strip_prefix(EHR_RESOURCE)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Whether `matched` is the creation of an EHR with no `ehr_id` in its path,
/// `POST {base}/v1/ehr`, routed only by the targeting headers (§12.4).
#[must_use]
pub fn creates_ehr(matched: &RouteMatch) -> bool {
    matched.group == EHR_GROUP && matched.operation_id == "ehr_create"
}

/// Whether `matched` is `GET {base}/v1/ehr`, the read of an EHR by subject.
#[must_use]
pub fn reads_ehr_by_subject(matched: &RouteMatch) -> bool {
    matched.operation_id == BY_SUBJECT
}

/// The `ehr_id` the path segment of `matched` decodes to, or `None` when it
/// is no `HIER_OBJECT_ID`.
#[must_use]
pub fn path_ehr_id(matched: &RouteMatch) -> Option<EhrId> {
    let param = matched.path_param(EHR_ID_PARAM)?;
    // NOTE: §12.5, a segment that is not UTF-8 or not a HIER_OBJECT_ID names
    // no EHR, so either failure is the malformed-ehr_id answer.
    EhrId::new(param.decoded().ok()?).ok()
}
