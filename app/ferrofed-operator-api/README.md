<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-operator-api

The wire documents of the gateway's operator surface and the operator
console's configuration resources: the gateway writes them and the operator
console reads them, so both sides compile against one definition.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

The documents name the registry's own types (`ferrofed-registry`: the ids,
the snapshot, the incidents and the replica) and add none of membership.
