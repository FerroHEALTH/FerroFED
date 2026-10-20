// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE IT Infrastructure (ITI) profiles a federation gateway or a patient
//! index binds, one feature per profile:
//!
//! - `pixm`: Patient Identifier Cross-reference for Mobile, ITI-83.
//! - `pdqm`: Patient Demographics Query for Mobile, ITI-78 and ITI-119.
//! - `mcsd`: Mobile Care Services Discovery, ITI-90 and ITI-91.
//! - `pmir`: Patient Master Identity Registry, ITI-93 and ITI-94.
//! - `xcpd`: Cross-Community Patient Discovery, ITI-55, the one profile on
//!   SOAP 1.2 with HL7 v3 and SAML XUA.
//! - `atna`: with `xcpd`, the ITI-55 audit message, written through the ATNA
//!   types of `ihe-atna` (ITI-20, the DICOM audit message over syslog with
//!   TLS).
//! - `balp`: with `pixm`, `pdqm`, `mcsd` or `pmir`, the audit records of that
//!   profile's transactions, the FHIR `AuditEvent` of the Basic Audit Log
//!   Patterns written through `ihe-atna`.
//!
//! The PIXm, PDQm, mCSD and PMIR clients authenticate with a credential the
//! HTTP client carries, or with an access token incorporated in each request
//! by an authorizer the caller supplies (module `authorizer`, IUA ITI-72).
//!
//! An audited PIXm, PDQm or XCPD exchange names whom it is made for, a user
//! or the client's own system (`ihe_atna::user`), and its audit record names
//! that user.
//!
//! The profiles are published at <https://profiles.ihe.net/ITI/>. The crate
//! depends on no application: it is the profiles' transactions as Rust, for
//! any caller. The profile modules land with their FerroFED issues (Annex A).
#![doc(test(attr(deny(warnings))))]

#[cfg(any(feature = "pixm", feature = "pdqm", feature = "mcsd", feature = "pmir"))]
pub mod authorizer;
#[cfg(feature = "mcsd")]
pub mod mcsd;
#[cfg(any(feature = "pixm", feature = "pdqm", feature = "mcsd", feature = "pmir"))]
pub mod outcome;
#[cfg(feature = "pdqm")]
pub mod pdqm;
#[cfg(feature = "pixm")]
pub mod pixm;
#[cfg(feature = "pmir")]
pub mod pmir;
#[cfg(any(
    all(feature = "balp", any(feature = "pixm", feature = "pdqm")),
    feature = "xcpd"
))]
pub mod recording;
#[cfg(any(
    feature = "pixm",
    feature = "pdqm",
    feature = "mcsd",
    feature = "pmir",
    feature = "xcpd"
))]
mod redact;
#[cfg(any(feature = "pixm", feature = "pdqm", feature = "mcsd", feature = "pmir"))]
mod search;
#[cfg(feature = "xcpd")]
pub mod xcpd;
