// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The testkit's own suite: the proxy, the seed builder and the harness PIX
//! Manager offline against stub nodes and the PIXm client, the unreachable
//! base, and the container harness behind the `FERROFED_E2E` gate.

mod e2e;
mod pix;
mod proxy;
mod seed;
mod unreachable;
mod xcpd;
