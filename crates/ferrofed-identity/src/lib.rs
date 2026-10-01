// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity roles of the Federation Tier as traits (resolver, localizer,
//! directory, consent pre-filter, onward authentication) and the patient
//! reference carrier, with no FHIR and no transport.
//!
//! The gateway core depends on these traits only; each binding implements
//! one in a crate of its own, so it can move without a change to the core
//! (docs/architecture.md section 6). This crate holds:
//!
//! - [`patient`]: [`PatientRef`](patient::PatientRef), the patient
//!   identifier as the gateway carries it, redacted everywhere (§5.4, N33);
//! - [`resolver`]: the [`Resolver`](resolver::Resolver) seam (N3, §5.2);
//! - [`dev`]: the static development cross-reference, FerroFED's own testing
//!   device, enabled only under the development profile.
//!
//! The localizer, directory, consent pre-filter and onward-authentication
//! seams, and the resolution step that drives them, land with their issues.
#![doc(test(attr(deny(warnings))))]

pub mod dev;
pub mod patient;
pub mod resolver;

// TODO(#43): the resolution step over the seams, and the localizer, directory and consent pre-filter traits.
