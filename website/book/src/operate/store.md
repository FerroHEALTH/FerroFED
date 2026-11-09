<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroFED's own store

`[store]` connects FerroFED to one PostgreSQL database of its own. The
store holds the state that must both survive a restart and be one on every
replica. No specification governs the store: it is FerroFED's own design.

## When you need it

You need `[store]` in two cases:

- an issuer is declared a national contact point
  (`[[federation.contact_point]]`,
  [Client authentication](authentication.md));
- the stored-query registry uses `backend = "postgres"`
  ([Stored queries](queries-and-areas.md#several-replicas-postgres)).

A configuration in either case that sets no `[store]` refuses to start, and
`config check` refuses it too, naming the key that needs the store and
`[store]`:

```text
federation.contact_point[0] needs FerroFED's own PostgreSQL store, and the configuration sets no [store]; add [store] with url or url_file
```

In every other case FerroFED runs with no PostgreSQL database. The embedded
`redb` file of the default stored-query backend and the read-only `files`
backend are unchanged.

## The database and its role

Give FerroFED a database and a login role of its own. Never point `[store]`
at a member CDR's database or role, and never at a schema inside a FerroEHR
cluster's database, so a gateway credential never reaches clinical data.
The tables live in the schema `ferrofed`. The role needs `CREATE` on the
database to make the schema, or a `ferrofed` schema made for it on which it
holds `CREATE` and `USAGE`. The store is tested against PostgreSQL 18.6.

## What it holds

The store holds only state that must outlive the process and be shared by
every replica:

| Table | What a row holds | When it goes |
|---|---|---|
| `ferrofed.stored_query_definition` | a stored-query definition of the `postgres` backend: name, version, the instant it was stored, its parameterised AQL (§12.7, N44) | never: a stored version is immutable |
| `ferrofed.emergency_access` | the emergency accesses granted to one health professional a national contact point relays, keyed by a SHA-256 digest of the professional's identity: the instants granted inside the window, and the end of that window ([The emergency access limit](../integrate/national-contact-point.md#the-emergency-access-limit)) | at `expires_at`, the end of the window of the last access granted, by the sweep |
| `ferrofed.dpop_proof` | one `DPoP` proof the gateway accepted, by the SHA-256 of its `jti` alone, never the value, so a replay to any replica is refused (RFC 9449 §11.1; [`DPoP`](authentication.md#dpop)) | at `expires_at`, the end of the window the proof is accepted in, by the sweep |
| `ferrofed.contact_point_dispensation` | who dispensed one dispensation a national contact point wrote, keyed by the SHA-256 of the gateway's id of it: the identity assertion's `organization-id` and the facility identifier the dispensation names on `performer.actor`, so its discard is held to that dispenser on every replica ([Dispensations and their discard](../integrate/national-contact-point.md#dispensations-and-their-discard)) | at `expires_at`, 24 hours after the dispensation, by the sweep; no discard is admitted after it |
| `ferrofed.store_migration` | one migration applied, and when | never |

These stay out of the store: caches (revocation and OCSP answers, issuer
keys), audit records (the audit spool), configuration (the revisions of
`[config]` keep their own store) and clinical data. A table that a later
release adds meets the same rule.

## The keys

```toml
[store]
url_file = "/run/secrets/ferrofed/store-url"   # or url = "...": the connection string, a secret
ca_file = "/etc/ferrofed/store-ca.pem"         # optional: the CA the database's certificate is verified by
max_connections = 4                            # 1 to 64
timeout = "10s"                             # one connection, or one statement
sweep_interval = "1min"                          # how often expired rows are deleted
```

| Key | Default | What it sets |
|---|---|---|
| `url`, `url_file` | none, one is required | the connection string, a URL (`postgres://ferrofed@db.example.org:5432/ferrofed?sslmode=require`) or libpq key/value pairs; a secret, read at boot and trimmed when it comes from the file, never both |
| `ca_file` | the platform's trusted roots | a PEM file of the CA certificates the database's certificate is verified by, in place of the platform's roots |
| `max_connections` | `4` | how many connections the gateway opens, 1 to 64 |
| `timeout_ms` | `10000` | how long one connection, or one statement, may take; zero is refused |
| `sweep_interval` | `1min` | how often the rows past their window are deleted; zero is refused |

Give the credentials with `url_file`: write the whole connection string,
password included, to a secret file only the gateway's user reads, and
keep `url` for a connection string that carries no password.

A key the table does not have refuses the configuration, naming it. A
connection string that does not parse is refused naming its key, never
quoting the string. The startup banner, the log and `ferrofed report` never
show the string. Changing `[store]` needs a restart.

## TLS is required

The store is reached over TLS under every profile, the development profile
included. The connection string must set `sslmode=require`; a string that
leaves `sslmode` unset or sets `disable` or `prefer` is refused by
`config check`, the start and a reload, naming `store.url` or
`store.url_file`:

```text
store.url does not require TLS: FerroFED's store is reached over TLS under every profile; set sslmode=require
```

TLS uses rustls and always verifies the server's certificate, against
`ca_file` when it is set and the platform's roots otherwise. A database that
listens on a Unix socket alone cannot serve TLS, so give it a TCP listener.

## Migrations

The migrations are part of the binary. At start, and each time a connection
opens, the gateway takes a PostgreSQL advisory lock in one transaction,
creates the schema `ferrofed` when it is absent, and applies each migration
`ferrofed.store_migration` does not record, in order. Two replicas that
boot together over one empty database apply each migration once: the second
waits for the lock and then finds every migration recorded. A database that
records a migration this release does not have was migrated by a newer
FerroFED: the start is refused, naming both numbers, and the database is
left as it was ([Rollback](rollback.md)). Migrations only move forward, and
a released migration never changes.

## Readiness and failure

The store has its own readiness indicator, `store`, which is down while the
database does not answer a statement within the readiness check's two
seconds ([Health probes](health.md)). A down indicator takes the replica out
of rotation.

Every probe asks the database, but the log tells you only what changed. The
first probe that finds the store down logs the warning `the store does not
answer` with the cause. While it stays down, `the store still does not
answer` repeats at most once every five minutes, with `down_for_s`. The
first probe that finds it answering again logs `the store answers again`, with
how long it was down. The five minutes are FerroFED's own choice.

A database that cannot be reached at start, or within `timeout_ms`, refuses
the start. Once running, a request that needs the store while the database
does not answer is refused, and is never answered from memory: a stored
query read or `PUT` answers `500`, names nothing in the body, and the log
names the failure.

## Expired rows

A table whose rows expire carries the end of each row's window in a column
`expires_at`. Every `sweep_interval`, the gateway deletes each row past
its window from every such table of the schema `ferrofed`, so the store
keeps a row no longer than its rule needs. The stored-query definitions have
no window and are never swept.

## Moving from `[stored_queries] url`

The `postgres` backend of the stored-query registry used to read its own
connection string, `[stored_queries] url` or `url_file`. Those keys are
gone, and a configuration that still sets either refuses to start, naming
the key and `[store]`. Move the connection string to `[store]`, point it at
the database the backend already uses, and set `sslmode=require`:

```toml
[stored_queries]
backend = "postgres"

[store]
url_file = "/run/secrets/ferrofed/store-url"
ca_file = "/etc/ferrofed/store-ca.pem"
```

The table of definitions is the store's first migration, so the gateway
takes over the existing table with every definition it holds, and a version
it held is refused a second `PUT` as before. [Upgrading](upgrading.md) lists
the step.
