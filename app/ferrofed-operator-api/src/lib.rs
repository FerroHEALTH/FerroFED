// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The wire documents of the gateway's operator surface and the operator
//! console's configuration resources.
//!
//! The gateway writes and the operator console reads every one of them, so
//! both sides compile against one definition and a body cannot drift from
//! its reader. The documents name the registry's own types
//! ([`ferrofed_registry`]) and hold no membership of their own.
//!
//! [`operator`] holds the reports the gateway's read-only operator surface
//! answers with, and [`health`] the body of the gateway's
//! `GET {base}/operator/dependencies`; each of them names the process that
//! produced it, a [`Replica`](ferrofed_registry::replica::Replica).
//! [`console`] holds a member the operator console manages beside the
//! document's, owned whole, and the report of who owns each configured
//! resource. [`resource`] holds the credentials and binding sections the
//! console manages whole, with their secrets as references. [`revision`]
//! holds the gateway's configuration revisions as its admin listener writes
//! them and the console reads them. [`running`] holds the running
//! configuration the admin listener answers: each deployment-wide setting
//! with its value, source and default. [`manufacturer`] names the
//! manufacturer of FerroFED, the one source the gateway and the operator
//! console both read it from. No specification governs the documents: our
//! own design.
#![doc(test(attr(deny(warnings))))]

pub mod console;
pub mod health;
pub mod manufacturer;
pub mod operator;
pub mod resource;
pub mod revision;
pub mod running;
