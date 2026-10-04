// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity roles of the Federation Tier as traits (resolver, localizer,
//! consent pre-filter), the patient reference carrier, and the adapters that
//! bind the roles and the registry's sources over the IHE binding crate.
//!
//! The traits and the carrier hold no FHIR and no transport, and the gateway
//! core depends on them only; each adapter implements a trait over a binding
//! crate, so a binding can move without a change to the core (§2.4, N27,
//! N27a). This crate holds:
//!
//! - [`patient`]: [`PatientRef`](patient::PatientRef), the patient
//!   identifier as the gateway carries it, redacted everywhere (§5.4, N33);
//! - [`resolver`]: the [`Resolver`](resolver::Resolver) seam (N3, §5.2);
//! - [`localizer`]: the [`Localizer`](localizer::Localizer) seam, which
//!   members might hold a patient's data (N4, §14.1);
//! - [`consent`]: the optional Step-1
//!   [`ConsentPrefilter`](consent::ConsentPrefilter) seam (N27a, §13.2.1);
//! - [`fhir`]: the HTTP client every IHE FHIR server is asked through, with
//!   its [`Authentication`](fhir::Authentication) and [`Tls`](fhir::Tls)
//!   material;
//! - [`pixm`]: the [`Resolver`](resolver::Resolver) over PIXm ITI-83 (#43);
//! - [`xcpd`]: the [`Localizer`](localizer::Localizer) over XCPD ITI-55
//!   (Annex A.3);
//! - [`nvi`]: the [`Localizer`](localizer::Localizer) over the NVI
//!   Localization Service of the Dutch Generic Functions (Annex B §B.1);
//! - [`mitz`]: the [`ConsentPrefilter`](consent::ConsentPrefilter) over
//!   Mitz's closed authorization question (Annex B §B.6);
//! - [`binding`]: the resolution bindings of §12.5.1 step 2, in memory and
//!   scoped to the client session (§12.5.1 step 2);
//! - [`lifecycle`]: what a PMIR ITI-93 message does to those bindings, and
//!   the ITI-94 subscriber that asks for the messages (track 8, Annex A.4);
//! - [`dev`]: the static development cross-reference and consent pre-filter,
//!   FerroFED's own testing devices, enabled only under the development
//!   profile;
//! - [`directory`]: the registry document in FHIR form, `Organization` and
//!   `Endpoint` resources read through `ihe_iti`'s mCSD reader (N19, N20),
//!   and the registry read from an mCSD directory and kept in step with it
//!   ([`directory::mcsd`], §15.1, N21);
//! - [`balp`]: the audit recorders of the PIXm, mCSD and PMIR transactions,
//!   the BALP `AuditEvent` sent to an ATNA Audit Record Repository over the
//!   FHIR Feed of ITI-20, or written to the audit log target.
//!
//! How the gateway authenticates to each node as itself (§13.1, N25) is the
//! engine's, in `ferrofed_engine::onward`.
#![doc(test(attr(deny(warnings))))]

pub mod atna;
pub mod balp;
pub mod binding;
pub mod consent;
pub mod dev;
pub mod directory;
pub mod fhir;
pub mod lifecycle;
pub mod localizer;
pub mod mitz;
pub mod nvi;
pub mod patient;
pub mod pixm;
pub mod resolver;
pub mod xcpd;
