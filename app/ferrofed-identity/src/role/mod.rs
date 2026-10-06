// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity roles every binding implements, and the patient reference
//! they are asked about.
//!
//! The seams hold no FHIR and no transport, and the gateway core depends on
//! them only (§2.4, N27, N27a). No specification governs the grouping: our
//! own design.

pub mod behalf;
pub mod consent;
pub mod demographics;
pub mod header;
pub mod localizer;
pub mod patient;
pub mod resolver;
