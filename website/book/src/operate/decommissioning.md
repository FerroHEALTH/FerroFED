<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Decommissioning

This page takes a FerroFED deployment out of service and removes what it
holds. Regulation (EU) 2024/2847, the Cyber Resilience Act, asks for
instructions on "the secure decommissioning of the product with digital
elements, including information on how user data can be securely removed"
(Annex II, point 8(d)), and
[CRA information to the user](../evaluate/cra-user-information.md#8d-secure-decommissioning)
points here. The order of the steps is our own design: no specification
governs it.

The gateway holds no clinical data. It keeps the routing state it learned in
memory, a few stores on disk, and the records waiting in its audit spools;
everything else it handed to another system, which keeps it
([What the gateway keeps](deployment-shape.md#what-the-gateway-keeps),
[Retention](../evaluate/data-protection.md#retention)).

## What a deployment holds

| What | Where | What happens to it |
|---|---|---|
| Resolution bindings, the `ehr_id` index, the learned `creating_system_id` routes | each replica's memory | lost when the replica stops; export the routes first to move them to a replacement |
| Stored-query definitions | the `[stored_queries]` backend: a `redb` file, the `ferrofed` schema of a PostgreSQL database, or read-only definition files | parameterised AQL, never a patient identifier; you remove it |
| Configuration revisions, under `config.ui = "managed"` | `config.path` (a `redb` file) or the `ferrofed` schema of the `config.url` database, and the secret files the console wrote to `config.secrets_dir` | every revision with who made it and who approved it, and the secret values set from the console; you remove them |
| IHE audit records and access records not yet delivered | the spools under `audit.repository.spool_dir` and `xcpd.audit_repository.spool_dir`, with their `quarantine` subdirectories | name patients and callers; delivered while a replica runs, then removed with the volume |
| Access and audit records delivered | your Audit Record Repository | kept for their retention period, never removed by decommissioning the gateway |
| Log lines, metrics and spans | your log, metrics and trace stores | kept as those stores keep them |
| The configuration, the registry document and the secrets | the files and the environment you give the gateway | you remove them, after the keys are retired |
| Operator console sessions | the console's memory | end when the console stops |

## 1. Move what a replacement needs

Skip this step when nothing replaces the gateway.

- **The learned routes.** Export them from the admin listener before the
  first replica stops; a stopped process has forgotten them
  ([Exporting the identity state](identity-state-export.md)). The bindings
  and the `ehr_id` index are not exported: a replacement learns them again.
- **The stored queries.** Copy the `redb` file while no gateway has it open,
  or dump the schema with `pg_dump --schema=ferrofed`, as before an upgrade
  ([Before you upgrade](upgrading.md#before-you-upgrade)).
- **The configuration.** Keep `ferrofed.toml` and the registry document.
  Under `config.ui = "managed"`, export the running revision as TOML
  ([Export and import](configuration-revisions.md#export-and-import)); the
  export names each secret by its file and never holds its value.

## 2. Let the audit spools deliver

A record in a spool exists nowhere else, and only a running gateway delivers
it. A lost spool loses records of transactions that did happen
([Losing a spool](container.md#losing-a-spool)).

1. Stop sending the gateway client traffic, so no new record is written.
2. Read `GET {base}/operator/dependencies` on each replica until each of
   `audit_repository` and `audit_feed` that it lists reads `up`. Each reads
   `degraded` while a record waits in its spool or sits in its quarantine
   ([The dependency report](health.md#the-dependency-report)).
3. A record in `quarantine` was refused by the repository and is never sent
   again. Read why it was refused before you remove it
   ([The spool and the failure policy](audit.md#the-spool-and-the-failure-policy)).

A replica stopped with records still waiting delivers them when it starts
again on the same volume.

## 3. Stop the replicas

Stop each replica with `SIGTERM`, so it drains: readiness answers `503`, the
requests in flight finish, and the bindings stop their processes
([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).

With `[pmir]`, the gateway holds a subscription at the Patient Identity
Registry. Set `on_drain = "unsubscribe"` on the last replica before it
stops, or on every replica that has its own `callback_url`; otherwise delete
the subscription at the Registry
([Several replicas](identity.md#several-replicas)). The drain logs a warning
when a binding did not finish stopping in
`server.bindings_drain_timeout_ms`, since what it holds at a remote service
may be left there; delete it at that service.

Stop the operator console as well. Its sessions live in its memory and end
with it ([The operator console](operator-console.md)).

## 4. Keep the records for their retention period

The access records are kept after the gateway is gone. Regulation (EU)
2025/327 keeps the information on each access "available for at least three
years from each date of access" (Art 9(2)), and each access record states
the first date it may be deleted, `ehds-retention-ends`, for your Audit
Record Repository to apply
([How long a record is kept](access-log.md#how-long-a-record-is-kept)).
Keep the repository, or move its records to the archive that replaces it,
until the last record's `ehds-retention-ends` has passed. The IHE audit
records follow your repository's own policy, and the log pipeline follows
your records of processing
([Retention](../evaluate/data-protection.md#retention)).

## 5. Retire the keys

The gateway signs the `openEHR-federation-client` token of every request
to a node and the client assertions of its `oauth2` grants with the key of
`[signing]`, a FAPI 2.0 grant's assertions with its `client_key_file`, and
publishes the public keys at `{base}/.well-known/jwks.json`
([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)).

1. Once the last replica has stopped, no token is signed. A token it signed
   lives at most `assertion_lifetime_s` or 60 s, whichever is longer. Where
   `jwks_uri` points somewhere other than the gateway, remove the gateway's
   keys there once that time has passed.
2. Ask each member to remove what it recorded when it admitted the gateway:
   the gateway's `jwks_uri` and the `iss` its tokens carry
   ([Verifying it at the node](caller-token.md#verifying-it-at-the-node)).
   A member that still trusts the keys would accept a token signed with a
   copy of them.
3. Ask each party that issued the gateway a credential to revoke it: the
   client each member's authorization server registered for an `oauth2`
   grant, the client certificate of mutual TLS
   ([Onward credentials](onward-credentials.md)), the credentials of the
   identity services, the PMIR feed token
   ([Signing keys and rotation](hardening.md#signing-keys-and-rotation)),
   and the client the operator console is registered as at its OpenID
   Connect provider ([The operator console](operator-console.md)).
4. Destroy the private keys and the secret files: every `[signing]` key
   file, each `client_identity_file`, each `client_key_file`,
   `previous_client_key_file` and `dpop_key_file`, and the files under
   `config.secrets_dir`.

## 6. Remove the stores and the volumes

- **The stored-query store and the revision store.** Delete the `redb`
  files. Both PostgreSQL backends keep their tables in the `ferrofed`
  schema; drop it, or the database, once nothing else you keep is in it.
- **The spool volumes.** Delete each replica's spool volume once step 2
  holds for it. The records in it name patients, so the volume sits on an
  encrypted disk ([The audit spool](hardening.md#the-audit-spool)); remove
  it as your storage removes an encrypted volume.
- **The configuration.** Delete the configuration file, the registry
  document and the environment that held `FERROFED__` values.
- **The images and binaries.** Delete them from your hosts and your
  registry mirror. A published release stays on the releases page and in
  the image registry; its removal from your environment is yours.
