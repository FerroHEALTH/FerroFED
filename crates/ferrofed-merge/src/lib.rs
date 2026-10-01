// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated merge: ORDER BY with LIMIT across nodes, DISTINCT,
//! version-identity dedup, decomposable aggregates and the completeness flag,
//! as pure functions over node result sets.
//!
//! Version 0.0.0 holds the crate in the workspace; the implementation lands
//! with FerroFED issue #52, following `docs/architecture.md` section 11.
#![doc(test(attr(deny(warnings))))]

// TODO(#52): the implementation this crate holds the place for.
