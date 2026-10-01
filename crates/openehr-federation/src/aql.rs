// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AQL rewrite of the Federation Tier (feature `aql`).
//!
//! The patient carrier is consumed at the gateway and each node query is
//! rewritten to that node's own `ehr_id`, as transformations of the openEHR
//! AQL syntax tree with no I/O. The implementation lands with FerroFED issue
//! #35, following `docs/architecture.md` section 4. The AQL release it
//! rewrites is [`crate::AQL`].

// TODO(#35): the rewrite this module holds the place for.
