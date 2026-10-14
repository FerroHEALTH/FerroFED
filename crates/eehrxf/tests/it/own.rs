// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary the component writes, read back through its own
//! receive path: decoded, held to the R4 document rules and to the EPS
//! `Bundle` and `Composition` profiles, its `Provenance` entries included.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::document::EPS_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use fhir_types::r4::resource::Resource;

use super::claimed::eu_mapping;
use super::document::document_mapped;
use super::support::eps;

/// The canonical URL of the EPS `Composition` profile.
const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

// FHIR R4 profiling, slicing rules: `bundle-eu-eps` slices `Bundle.entry` open, and FHIR R4
// References, logical references: a `Provenance.entity.what` by identifier alone is valid.
#[test]
fn the_summary_the_component_writes_is_received_by_it() -> Result<(), Box<dyn Error>> {
    let bundle = document_mapped(&eu_mapping()?, 2)?;
    let provenance = bundle
        .entry
        .iter()
        .filter(|entry| matches!(entry.resource, Some(Resource::Provenance(_))))
        .count();
    assert_eq!(2, provenance, "one Provenance per mapped run");
    let text = serde_json::to_string(&bundle)?;
    let document = ReceivedDocument::read(&text)?;
    document.check(&eps(EPS_BUNDLE)?, &eps(EPS_COMPOSITION)?)?;
    Ok(())
}
