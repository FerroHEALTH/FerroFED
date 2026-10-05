// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The testkit's own suite: the proxy, the seed builder, the harness PIX
//! Manager, the harness PDQm Supplier and the harness care services directory
//! offline against stub nodes and the IHE clients, the operator console
//! against a running gateway, the unreachable base, and the container
//! harness behind the `FERROFED_E2E` gate.

mod console;
mod e2e;
mod mcsd;
mod pdq;
mod pix;
mod pmir;
mod proxy;
mod seed;
mod tls;
mod unreachable;
mod xcpd;
