<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Upgrading

This page says how to move a gateway to a newer release, what each release
asks you to change, and which configuration changes FerroFED makes without
notice and which it never does. [Rollback](rollback.md) says how to go back.
No specification governs this page: our own design.

## Before you upgrade

1. Read the release's **Upgrade notes**, the first section of its
   [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
   entry. They list every change you must make: a key that became
   mandatory, a value that is now refused, a key renamed, a file to move.
   The notes for the releases so far are below.
2. Keep a copy of the configuration you run now, `ferrofed.toml` and the
   registry document, and note the image digest or binary version. A
   rollback needs both.
3. Back up the stored-query store when `[stored_queries]` is set: copy the
   `redb` file while no gateway has it open, or dump the `ferrofed` schema
   of the PostgreSQL database (`pg_dump --schema=ferrofed`). The new release
   may migrate the store, and an older release refuses a store a newer one
   migrated.
4. Run the new binary's `config check` over the configuration, with the
   upgrade notes applied. It reads every file and secret as `serve` does,
   binds no socket, and exits `78` naming each key it refuses. With the
   image:

   ```text
   docker run --rm --volume ./ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro \
     --volume ./secrets:/run/secrets/ferrofed:ro \
     ghcr.io/ferrohealth/ferrofed:<new version> config check
   ```

   The warnings it prints name every deprecated key you still set, and the
   release that will refuse it.

Then roll the new release out: `docker compose up --wait` with the new
`compose.yaml`, or a rolling update of the StatefulSet. Readiness answers
`503` until the new process has loaded the configuration and the store.

## Several replicas on one PostgreSQL store

A replica migrates the shared stored-query store when it starts. Once one
replica of a release that brings a new schema version has started, a
replica of the older release refuses its next connection to the store
([Rollback](rollback.md#the-stored-query-store)). When the upgrade notes
name a new schema version, upgrade every replica in one rollout and expect
the stored-query routes of the old replicas to answer `500` until they are
replaced. A release whose notes name none can be rolled out replica by
replica.

## The compatibility policy

FerroFED is in its `0.0.x` series, and each release can change the
configuration. These rules bound how.

- **A change that stops a configuration the last release accepted is in
  the Upgrade notes.** A key made mandatory, a value now refused, a table
  moved: each is listed with what to set instead. CI runs the new build's
  `config check` over the example configuration the last release attached,
  and refuses a change it rejects unless an upgrade note is pending.
- **A key is never renamed or removed at once.** For at least one release
  the old key is still read, under its new name, and `config check` and the
  start-up log warn that it is deprecated and name the release that will
  refuse it. Setting the old and the new key together is refused, so the
  two cannot disagree. The Upgrade notes name the rename in the release
  that deprecates the key and again in the release that refuses it.
- **A new key is optional, or the Upgrade notes say otherwise.** A release
  that adds a key gives it a default that keeps the behaviour of the
  release before, unless the change is a security fix that cannot keep it.
- **An older release refuses a key it does not know.** Every table refuses
  unknown keys, so a gateway rolled back to an earlier release refuses a key
  added since. Remove the keys the newer release added before rolling back
  ([Rollback](rollback.md#the-configuration)).
- **The environment follows the file.** A `FERROFED__` override names the
  same key as the file, so a rename applies to it too, and the warning
  names the dotted key.
- **Stored data migrates forward only.** A stored-query store records its
  schema version, is migrated forward in order when the gateway starts, and
  is never migrated back. A release refuses to start over a store a newer
  release migrated.

Security fixes go to the latest release only ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md)),
so stay within a release or two of the newest.

## Upgrade notes by release

### To v0.0.9, from v0.0.8

- If you run the operator console with `[session] secure_cookie = false`,
  either set it to `true` or make `[oidc] redirect_uri` an `http` URL on a
  loopback host: the console refuses any other combination (#615).
- A stored-query store that holds a definition naming its patient by a
  literal, written past the gateway by a restore or a manual insert, refuses
  the start (#586). Remove that row, or the file, and store the definition
  again with a `$parameter`.
- The released binary and image carry both bindings, as before. A build of
  your own with `--no-default-features` needs `--features binding-ihe` or
  `binding-nl` for the sections it uses, or refuses them as unknown keys
  (#489).
- The `configuration resolved` log line no longer carries the registry
  directory, PIX Manager, PMIR and PDQm fields; they are on the IHE
  binding's own `binding configured` line (#489). Update a log query that
  reads them.

### To v0.0.8, from v0.0.7

- **Every caller authenticates.** A gateway that federates refuses to start
  without an `[auth]` table that trusts at least one issuer
  (`[[auth.issuer]]` and `auth.audience`), and every request to the
  ITS-REST surface and `OPTIONS {base}/` needs an access token from that
  issuer with a SMART on openEHR scope and a purpose of use, unless
  `auth.purpose_of_use.required = false` (#80;
  [Client authentication](authentication.md)). Issue tokens to your clients
  before you upgrade.
- **`[signing]` is mandatory** whenever a registry is configured, by
  `registry.document` or `[registry.mcsd]`: add a P-384 or P-256 signing key
  and the `jwks_uri` it is published at, and make
  `{base}/.well-known/jwks.json` reachable from the nodes' authorization
  servers ([What a node is told about the caller](authentication.md#what-a-node-is-told-about-the-caller)).
- **`[audit] destination`** is mandatory for a production gateway with
  `[pixm]`, `[registry.mcsd]` or `[pmir]`. Set `"log"` to keep the
  behaviour of v0.0.7 while you add an Audit Record Repository; `"off"` is
  refused outside development (#486; [The audit trail](audit.md)).
- **`[xcpd] audit`** is mandatory: `"log"`, `"repository"`, or `"off"`
  under the development profile alone (#410). With `"repository"`,
  `[xcpd.audit_repository] spool_dir` is mandatory outside development and
  `[xcpd] home_community` is mandatory.
- The ADMIN API under `{base}/v1/admin/` answers `403
  operation-refused`, where it answered `501`.

### Unreleased

The pending notes are the `*.upgrade.md` files under
[`changelog.d/`](https://github.com/FerroHEALTH/FerroFED/tree/main/changelog.d),
which the next release lists under its Upgrade notes.
