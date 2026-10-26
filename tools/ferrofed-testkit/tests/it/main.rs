// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The testkit's own suite: the proxy, the seed builder and the harness care
//! services directory
//! offline against stub nodes and the IHE clients, the operator console
//! against a running gateway, the vendored MyHealth@EU, eHDSI and Xt-EHR
//! material the architecture cites, the unreachable base, the container harness
//! behind the `FERROFED_E2E` gate, and the operator console's browser
//! journeys behind the `FERROFED_JOURNEYS` gate.
#![cfg_attr(
    target_os = "macos",
    allow(
        linker_messages,
        reason = "Apple's ld reports that the unoptimized binary's __eh_frame is over the 16 MB its compact unwind table can encode, so a panic unwinds through the DWARF tables at some cost in speed; only that linker prints it, and only past that size"
    )
)]

mod console;
mod crossref;
mod e2e;
mod journeys;
mod keycloak_recipe;
mod mcsd;
mod myhealth_eu;
mod national_connector;
mod pmir;
mod proxy;
mod seed;
mod tls;
mod unreachable;
mod xcpd;
