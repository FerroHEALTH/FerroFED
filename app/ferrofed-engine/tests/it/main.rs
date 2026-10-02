// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the crate boundaries, the pinned releases, node
//! dispatch against a mock node (§11.1, N16, N28, N33), the fan-out under both
//! completion strategies against mock nodes (§11.3 to §11.5, N37, N38), the
//! Tier order and `LIMIT` over the node answers (§11.6.1, N39), and
//! single-node forwarding (§7a.3, N22, N31, N33).

mod architecture;
mod dispatch;
mod fanout;
mod forward;
mod gate;
mod pins;
