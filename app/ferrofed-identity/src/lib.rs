// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity roles of the Federation Tier as traits (resolver, localizer,
//! directory, consent pre-filter, onward authentication) and the patient
//! reference carrier, with no FHIR and no transport.
//!
//! The gateway core depends on these traits only; each binding implements
//! one in a crate of its own, so it can move without a change to the core
//! (§2.4, N27, N27a). This crate holds:
//!
//! - [`patient`]: [`PatientRef`](patient::PatientRef), the patient
//!   identifier as the gateway carries it, redacted everywhere (§5.4, N33);
//! - [`resolver`]: the [`Resolver`](resolver::Resolver) seam (N3, §5.2);
//! - [`localizer`]: the [`Localizer`](localizer::Localizer) seam, which
//!   members might hold a patient's data (N4, §14.1);
//! - [`pixm`]: the [`Resolver`](resolver::Resolver) over PIXm ITI-83 (#43);
//! - [`binding`]: the resolution bindings of §12.5.1 step 2, in memory and
//!   scoped to the client session (§12.5.1 step 2);
//! - [`dev`]: the static development cross-reference, FerroFED's own testing
//!   device, enabled only under the development profile;
//! - [`directory`]: the registry document in FHIR form, `Organization` and
//!   `Endpoint` resources read through `ihe_iti`'s mCSD reader (N19, N20).
//!
//! The consent pre-filter and onward-authentication seams land with their
//! issues.
#![doc(test(attr(deny(warnings))))]

pub mod binding;
pub mod dev;
pub mod directory;
pub mod localizer;
pub mod patient;
pub mod pixm;
pub mod resolver;

// TODO(#83): the consent pre-filter seam, then the directory sync (#86).
