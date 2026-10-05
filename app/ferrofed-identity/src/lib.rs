// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity roles of the Federation Tier as traits (resolver, localizer,
//! consent pre-filter), the patient reference carrier, and the adapters that
//! bind the roles and the registry's sources over the binding crates.
//!
//! The traits and the carrier hold no FHIR and no transport, and the gateway
//! core depends on them only; each adapter implements a trait over a binding
//! crate, so a binding can move without a change to the core (§2.4, N27,
//! N27a). The crate is laid out by role and by binding, one folder per
//! binding behind its feature (no specification governs the layout: our own
//! design):
//!
//! - [`role`]: the seams every binding implements, and the patient they are
//!   asked about:
//!   - [`role::patient`]: [`PatientRef`](role::patient::PatientRef), the
//!     patient identifier as the gateway carries it, redacted everywhere
//!     (§5.4, N33);
//!   - [`role::behalf`]: [`OnBehalfOf`](role::behalf::OnBehalfOf), whom each
//!     role is asked for, the verified caller or the gateway itself, which an
//!     audited binding names in its audit record;
//!   - [`role::resolver`]: the [`Resolver`](role::resolver::Resolver) seam
//!     (N3, §5.2);
//!   - [`role::localizer`]: the [`Localizer`](role::localizer::Localizer)
//!     seam, which members might hold a patient's data (N4, §14.1);
//!   - [`role::consent`]: the optional Step-1
//!     [`ConsentPrefilter`](role::consent::ConsentPrefilter) seam (N27a,
//!     §13.2.1);
//!   - [`role::demographics`]: the optional
//!     [`Demographics`](role::demographics::Demographics) seam, the master
//!     identity of an identifier the cross-reference does not map (Annex A
//!     §A.2);
//! - `ihe` (feature `ihe`): the adapters of the IHE binding over `ihe_iti`
//!   (Annex A):
//!   - `ihe::pixm`: the resolver and localizer over PIXm ITI-83;
//!   - `ihe::pdqm`: the demographics step over PDQm ITI-78 or ITI-119
//!     (Annex A §A.2);
//!   - `ihe::xcpd`: the localizer over XCPD ITI-55 (Annex A.3);
//!   - `ihe::pmir`: what a PMIR ITI-93 message does to the resolution
//!     bindings, and the ITI-94 subscriber that asks for the messages
//!     (track 8, Annex A.4);
//!   - `ihe::mcsd`: the registry document in FHIR form, `Organization` and
//!     `Endpoint` resources read through `ihe_iti`'s mCSD reader (N19, N20),
//!     and in `ihe::mcsd::source` the registry read from an mCSD directory
//!     and kept in step with it (§15.1, N21);
//!   - `ihe::audit`: the ITI-55 audit messages over ITI-20 syslog
//!     (`ihe::audit::atna`), and the BALP `AuditEvent` of the PIXm, PDQm,
//!     mCSD and PMIR transactions over the FHIR Feed of ITI-20 or to the
//!     audit log target (`ihe::audit::balp`);
//! - `nl` (feature `nl`): the adapters of the Dutch binding over
//!   `nl_generic_functions` (Annex B), and the custodian map both read:
//!   - `nl::nvi`: the localizer over the NVI Localization Service (Annex B
//!     §B.1);
//!   - `nl::mitz`: the consent pre-filter over Mitz's closed authorization
//!     question (Annex B §B.6);
//! - [`session`]: the resolution bindings of §12.5.1 step 2, in memory and
//!   scoped to the client session;
//! - [`fhir`]: the one HTTP client build every outbound client of this crate
//!   uses, with its [`Authentication`](fhir::Authentication) and its one
//!   [`Tls`](fhir::Tls) type;
//! - [`dev`]: the static development cross-reference and consent pre-filter,
//!   FerroFED's own testing devices, enabled only under the development
//!   profile.
//!
//! How the gateway authenticates to each node as itself (§13.1, N25) is the
//! engine's, in `ferrofed_engine::onward`.
#![doc(test(attr(deny(warnings))))]

pub mod dev;
pub mod fhir;
#[cfg(feature = "ihe")]
pub mod ihe;
#[cfg(feature = "nl")]
pub mod nl;
pub mod role;
pub mod session;
