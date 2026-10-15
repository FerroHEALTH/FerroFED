// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The code systems a coded element is transcoded into, for the report of
//! each semantic transformation the face applies (eHealth Network General
//! Guidelines Release 3.1 Art 6(3)).
//!
//! A coding is transcoded when its system is one of the preferred code
//! systems of the eHealth Network Guidelines on Patient Summary §4
//! ([`eehrxf::terminology::ehn`]), or an archetype-local code system the
//! component renders from the operational template, which the terminology
//! rules count as transcoded too. Every other coded element travels under
//! Art 10(7) of that guideline.

use eehrxf::terminology::ehn;
use eehrxf::terminology::system::ARCHETYPE_LOCAL;

/// Returns whether a coding of `system` is transcoded.
#[must_use]
pub fn preferred(system: &str) -> bool {
    ehn::PATIENT_SUMMARY.contains(&system) || system.starts_with(ARCHETYPE_LOCAL)
}
