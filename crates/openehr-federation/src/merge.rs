// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated merge of node answers (feature `merge`).
//!
//! ORDER BY with LIMIT across nodes, DISTINCT, version-identity dedup,
//! decomposable aggregates and the completeness flag, as pure functions over
//! node result sets. The implementation lands with FerroFED issue #52,
//! following `docs/architecture.md` section 9.

// TODO(#52): the merge this module holds the place for.
