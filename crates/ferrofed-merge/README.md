<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-merge

The federated merge: ORDER BY with LIMIT across nodes, DISTINCT, version-identity dedup, decomposable aggregates and the completeness flag, as pure functions over node result sets.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

Version 0.0.0 holds the crate in the workspace. The implementation lands with
[FerroFED issue #52](https://github.com/rubentalstra/FerroFED/issues/52),
and the design is recorded in the repository's architecture document.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
