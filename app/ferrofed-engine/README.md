<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-engine

The federation engine: dispatch and fan-out to each node over ITS-REST, the per-node and overall budgets, the completeness decision and the outbound identifier-hygiene gate. Follow-up routing on the creating system id is planned.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

An application crate under `app/`: FerroFED's own glue, never published. The
`dispatch` module holds node dispatch (#34): one `openehr-its` client per
registry endpoint and the mapping from a node's answer to its §11.1 endpoint
status. The `fanout` module (#37) sends one request per in-scope node under
one deadline, builds `meta.federation` from every outcome and applies the
all-or-nothing decision: `504` for an unanswered node, `424` for a node error,
`200` otherwise. The `hygiene` module (#45) is the outbound gate every request
to a node passes before it is sent: it refuses a request that still carries a
patient identifier resolution consumed (§5.4.1, N33). Follow-up routing on the
creating system id (§12) is planned (#61 to #66). The design is recorded in
the repository's architecture document.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
