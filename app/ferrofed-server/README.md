<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-server

The FerroFED server: the `ferrofed` binary, the openEHR federation gateway
behind one ITS-REST façade.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

A thin `main.rs` runs over the library run path: `ferrofed serve` and
`ferrofed config check`, the TOML and environment configuration, telemetry,
the request log, the health family and the bounded drain on `SIGTERM`. The
storage implementations live in this crate and nowhere else.

An application crate under `app/`: never published.

## Licence

Business Source License 1.1 (`LICENSE` at the repository root): free for every
non-production use and for non-commercial production use; a commercial licence
for other production use; Apache License 2.0 four years after each version.
