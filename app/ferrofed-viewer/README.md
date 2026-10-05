<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-viewer

The FerroFED operator console: the `ferrofed-viewer` binary, a Leptos app
rendered on the server and hydrated in the browser, with a
backend-for-frontend in front of the gateway.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

The console is one more client of the gateway's public surface. It calls the
gateway over HTTP with the signed-in operator's own access token, holds the
sign-in session on its server, and holds no clinical data. Built so far: the
health route, the landing page, the OpenID Connect sign-in redirect with PKCE
and its server-side session, and the typed gateway client. The operator views
(#276) and the query console (#277) are planned.

Build it with cargo-leptos, from this directory:

```sh
cargo leptos build --release
```

That writes the site bundle under `target/site` and the server binary under
`target/release`. `ferrofed-viewer --config viewer.toml serve` runs it,
`config check` checks a configuration without binding a socket, and
`healthcheck` is the container probe.

An application crate under `app/`: never published.

## Licence

Business Source License 1.1 (`LICENSE` at the repository root): free for every
non-production use and for non-commercial production use; a commercial licence
for other production use; Apache License 2.0 four years after each version.
