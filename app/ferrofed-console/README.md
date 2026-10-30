<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-console

The FerroFED Console, the operator console: the `ferrofed-console` binary,
whose server draws every page whole with FerroUI's components, with a
backend-for-frontend in front of the gateway.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

The console is one more client of the gateway's public surface. It calls the
gateway over HTTP with the signed-in operator's own access token, holds the
sign-in session on its server, and holds no clinical data. Built so far: the
health route, the shell (a top bar with the gateway's status, a light, dark
and system theme the server draws, and the operator's menu; a grouped
navigation that is a drawer on a narrow screen; and an About page that names
the manufacturer), the sign-in page, OpenID Connect sign-in with PKCE, the code
exchange and the ID Token check, the server-side session, the typed gateway
client, and the operator views: the members and their health, the integrity
incidents and the `creating_system_id` routing table, the stored queries, and
the self-description. The query console runs an AQL or a stored query through
the gateway and shows every node's status beside the rows.

Build it from the repository root with `cargo build --release -p
ferrofed-console`: the binary serves FerroUI's stylesheet, themes and script
modules itself, so there is no WebAssembly and no site bundle to build.
`ferrofed-console --config console.toml serve` runs it,
`config check` checks a configuration without binding a socket, and
`healthcheck` is the container probe.

An application crate under `app/`: never published.

## Licence

Business Source License 1.1 (`LICENSE` at the repository root): free for every
non-production use and for non-commercial production use; a commercial licence
for other production use; Apache License 2.0 four years after each version.
