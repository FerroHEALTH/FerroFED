<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Rollback

This page says what going back to an earlier release does to each thing
the gateway keeps, and how to do it. [Upgrading](upgrading.md) says what to
keep before an upgrade so that a rollback is possible. No specification
governs this page: our own design.

The gateway holds no clinical data. What a rollback has to care about is
the configuration, the stored-query store and the audit spools. The
resolution bindings, the `ehr_id` index and the learned `creating_system_id`
routes live in memory and are rebuilt after any restart.

## The configuration

Every table refuses a key it does not know, so an earlier release refuses
any key added since. Go back to the configuration you kept before the
upgrade, or remove each key the newer release added: the newer release's
changelog lists them under Added, and an earlier release's `config check`
names each one it refuses.

```text
docker run --rm --volume ./ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro \
  --volume ./secrets:/run/secrets/ferrofed:ro \
  ghcr.io/ferrohealth/ferrofed:<earlier version> config check
```

A key the newer release renamed was still read under its old name for at
least one release ([the compatibility policy](upgrading.md#the-compatibility-policy)),
so a configuration that uses the old name works on both sides of that
release.

## The stored-query store

A writable store records its schema version: the `redb` file in its
`ferrofed_schema` table, PostgreSQL in `ferrofed.schema_version`. A release
migrates the store forward when it starts, and refuses to start over a store
at a version newer than it knows, with a message that names both versions:

```text
the redb stored-query store is at schema version 2, and this FerroFED knows
versions up to 1: a newer FerroFED wrote it; run that version, or restore the
backup taken before the upgrade
```

It never rewrites that store. To roll back across a schema version, stop
every gateway, restore the backup you took before the upgrade (the `redb`
file, or the `ferrofed` schema with `pg_restore`), and start the earlier
release. A definition stored after the upgrade is lost with the restore;
store it again. To keep the newer definitions instead, stay on the newer
release.

Schema version 1 is the layout every release so far has written. Releases
before the version was recorded open the `redb` file and the PostgreSQL
table as before and leave the version table alone, so a rollback to one of
them needs no restore while the store is at version 1.

With several replicas on one PostgreSQL store, a replica of the earlier
release refuses its next connection once a newer one has migrated the
store. Roll every replica back together, after the restore.

## The audit spools

The ATNA spools (`[audit] spool_dir`, `[xcpd.audit_repository] spool_dir`)
hold the audit records the gateway has not yet delivered. Before a rollback,
let them drain: wait until `GET {base}/health/dependencies` reads `up` for
the audit repository, then stop the gateway. A spool the earlier release
cannot read is not lost: a file it does not recognise refuses the start,
naming the file, and a record it cannot parse is moved to the spool's
`quarantine` directory ([The audit trail](audit.md)).

## The image and the binary

Pin the earlier release by its version tag, or better its digest, in
`compose.yaml` (`FERROFED_VERSION`) or in the StatefulSet, and roll it out
as you rolled out the upgrade. Every release attaches the example
configuration it accepts, which is a starting point for the configuration
you go back to.
