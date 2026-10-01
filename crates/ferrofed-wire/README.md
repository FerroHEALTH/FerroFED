<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-wire

The federation wire additions of the Federation Tier with AQL specification: `meta.federation`, the endpoint status vocabulary and the federation headers, as typed carriers held to the published schemas.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

Version 0.0.0 holds the crate in the workspace. The implementation lands with
[FerroFED issue #33](https://github.com/rubentalstra/FerroFED/issues/33),
and the design is recorded in the repository's architecture document.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
