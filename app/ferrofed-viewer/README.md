<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-viewer

The FerroFED Console, the operator console: the `ferrofed-viewer` binary, a Leptos app
rendered on the server and hydrated in the browser, with a
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

Build it with cargo-leptos, from the repository root:

```sh
scripts/release/viewer-tools.sh "$HOME/.cache/ferrofed-viewer-tools"
export PATH="$HOME/.cache/ferrofed-viewer-tools/bin:$PATH"
scripts/release/viewer-site.sh --release
cargo build --release -p ferrofed-viewer
```

The first line installs the Tailwind CSS CLI and wasm-opt at their pinned
releases, each checked against its SHA-256, and the site script refuses to
run cargo-leptos unless the ones on `PATH` match those digests, so it never
downloads either itself. That writes the site bundle under `target/site`
and the server binary under `target/release`.
`ferrofed-viewer --config viewer.toml serve` runs it,
`config check` checks a configuration without binding a socket, and
`healthcheck` is the container probe.

An application crate under `app/`: never published.

## Licence

Business Source License 1.1 (`LICENSE` at the repository root): free for every
non-production use and for non-commercial production use; a commercial licence
for other production use; Apache License 2.0 four years after each version.
