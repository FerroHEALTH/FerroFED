// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Integration tests: the crate boundaries, the pinned releases, node
//! dispatch against a mock node (§11.1, N16, N28, N33), and the fan-out with
//! its all-or-nothing decision against mock nodes (§11.3 to §11.5, N37, N38).

mod architecture;
mod dispatch;
mod fanout;
mod pins;
