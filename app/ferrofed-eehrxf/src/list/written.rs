// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entry of a report the gateway writes from a member's composition on
//! read ([`Written`]), and the document type and priority category each
//! category is listed under.

use fhir_types::r4::attachment::Attachment;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::DocumentReference;

use super::{DISCHARGE_SUMMARY, IMAGING_REPORT, Kind, LABORATORY_REPORT, Listing, URI_SYSTEM};
use crate::face::FHIR_JSON;
use crate::report::Written;
use crate::summary::Organisation;

/// A laboratory report.
const LABORATORY: Kind = Kind {
    code: LABORATORY_REPORT,
    display: "Laboratory report",
    category: "Laboratory-Reports",
    category_display: "medical test results, including laboratory and other diagnostic results and related reports",
};

/// An imaging report, the `Diagnostic imaging report` code of the
/// `eehrxf-document-type-medical-imaging-vs` value set.
const IMAGING: Kind = Kind {
    code: IMAGING_REPORT,
    display: "Diagnostic imaging report",
    category: "Medical-Imaging",
    category_display: "medical imaging studies and related imaging reports",
};

/// A discharge report, the `Discharge summary` code of the
/// `eehrxf-document-type-discharge-report-vs` value set; the `34105-7`
/// `composition-eu-hdr` fixes is not in it.
const DISCHARGE: Kind = Kind {
    code: DISCHARGE_SUMMARY,
    display: "Discharge summary",
    category: "Discharge-Reports",
    category_display: "discharge reports",
};

/// Returns the entry of a report of the category `written` the gateway
/// writes on read.
///
/// The entry has the logical id `id` and the unique id `unique`, a
/// `urn:uuid:` URN, and names the report of a composition of the member
/// `custodian` at `url`, a URL the gateway signs relative to the FHIR base.
///
/// The document is in the category's `Bundle` profile
/// ([`Written::bundle`]): an HL7 Europe Laboratory Report 2.0.0
/// `Bundle-eu-lab` of the `eehrxf-document-type-laboratory-report-vs` type,
/// an HL7 Europe Hospital Discharge Report 1.0.0 `bundle-eu-hdr` of the
/// `eehrxf-document-type-discharge-report-vs` type, or an HL7 Europe Imaging
/// Report 1.0.0-ballot `BundleReportEuImaging` of the
/// `eehrxf-document-type-medical-imaging-vs` type, named by the identifier
/// its `Bundle.identifier` carries when it is read.
#[must_use]
pub fn entry(
    written: Written,
    (id, unique): (&str, &str),
    listing: Listing<'_>,
    custodian: &Organisation,
    url: &str,
) -> DocumentReference {
    let (kind, title) = match written {
        Written::Discharge => (
            DISCHARGE,
            "Discharge report written by the gateway from the member's record",
        ),
        Written::Imaging => (
            IMAGING,
            "Imaging report written by the gateway from the member's record",
        ),
        Written::Laboratory => (
            LABORATORY,
            "Laboratory report written by the gateway from the member's record",
        ),
    };
    let attachment = Attachment {
        content_type: Some(FHIR_JSON.into()),
        url: Some(url.into()),
        title: Some(title.into()),
        ..Attachment::default()
    };
    let format = Coding {
        system: Some(URI_SYSTEM.into()),
        code: Some(written.bundle().into()),
        ..Coding::default()
    };
    super::entry(
        (id, (URI_SYSTEM, unique)),
        (kind, listing),
        custodian,
        (attachment, Some(format)),
    )
}
