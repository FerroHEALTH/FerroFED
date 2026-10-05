// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The calls that send one request to one node, outside the fan-out of a
//! federated query.
//!
//! [`ehr`] creates and reads an EHR on one node for the admission check
//! (§12b.1), [`forward`] passes one client request to its owning node once
//! (§7a.3, N22, N31), and [`probe`] sends the one read of §12.5.1 step 4 to
//! each member. No specification governs the grouping: our own design.

pub mod ehr;
pub mod forward;
pub mod probe;
