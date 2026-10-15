<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuration revisions

With `config.ui = "managed"`, the gateway keeps its configuration as a
history of revisions in a store, and you save, compare, apply and roll back
revisions on the admin listener. With `config.ui = "off"`, the default, none
of this exists: the gateway is configured by its file and the `FERROFED__`
environment alone, opens no store, and serves no revision route. No
specification governs this page: our own design.

What is built is the revision store and its API on the admin listener
(#576), and the members and stored queries the operator console manages,
with their owners (#577, [Console-managed resources](#console-managed-resources)).
Credentials and bindings with a confirmation step and a second operator's
approval are planned (#578), then export to and import from TOML (#579).

## Turning it on

```toml
[metrics]
listen = "127.0.0.1:9464"     # the admin listener, which serves the revisions

[config]
ui = "managed"                # "off", the default, changes nothing
backend = "redb"              # or "postgres", for replicas that share a store
path = "/var/lib/ferrofed/config-revisions.redb"   # redb: the store file
# url_file = "/run/secrets/ferrofed/config-revisions-url"   # postgres: the connection string, a secret
```

| Key | Default | What it sets |
|---|---|---|
| `config.ui` | `off` | `managed` keeps the configuration as revisions on the admin listener |
| `config.backend` | `redb` | `redb`, an embedded file one gateway process opens at a time, or `postgres`, one database every replica shares; `postgres` needs a build with the `postgres` feature |
| `config.path` | none | the `redb` store file, created when it does not exist |
| `config.url`, `config.url_file` | none | the PostgreSQL connection string of the `postgres` backend, a URL or libpq key/value pairs; a secret, never logged, and its `sslmode` must be `require` when it carries a password over a network outside the development profile |

Under `off`, `GET /admin/config` answers `404 configuration-unmanaged`, so a
client tells an unmanaged gateway from a wrong admin URL. Under `off`, any other key of `[config]` refuses to start, so a store you
meant to use is never silently ignored. Under `managed`, `[metrics] listen`
must be set, since the revision API is served on the admin listener.

## What a revision holds

A revision is the whole configuration less its bootstrap core: the file and
the environment as the gateway reads them, written as TOML text. The
bootstrap core comes from the file alone and is never part of a revision:

| Bootstrap core | Why it stays in the file |
|---|---|
| `profile` | it decides the transport policy every outbound URL is held to |
| `[server]` | the client listener: its address, TLS, base path and limits |
| `[metrics]` | the admin listener and its scrape authentication |
| `[auth]` | client authentication, the operator scope among it |
| `[config]` | the revision store itself |
| `[dev]` | the development cross-reference pairs patient identifiers with `ehr_id`s, which no store holds |

So a revision can neither lock you out of the admin listener nor lower the
security floor the file sets. A revision that sets a key of the core is
refused.

A revision never holds a secret value. Under `managed`, every secret
outside the core is written as its `_file` reference, such as
`bearer_token_file`, and the gateway refuses to start, and refuses a
revision, that writes one inline (`bearer_token`, `password`,
`client_identity`, `client_secret`, `assertion`, `feed_token`,
`stored_queries.url`, or a URL that carries a password). The refusal names
the key, never the value. A secret inside the core, such as
`metrics.scrape_token`, may stay inline, since the core is never stored.
Comments are not kept: a revision is stored as TOML writes it.

Each revision records its number, the revision it was based on, where it
came from (`file`, `operator` or `rollback`), who made it (the file, or the
operator's issuer and subject as their token names them), when, and why.

## The file is a revision too

At start, and at each `SIGHUP` reload, the gateway reads its file and
environment and checks them as `config check` does. When the part a
revision holds differs from the newest revision the file gave, it is saved
as a new revision by the file and runs. When it does not, the revision
marked running keeps running, with the file's bootstrap core beside it,
so a revision you applied survives a restart until you change the file.
Nothing ever writes back to the file.

## Every revision is checked as `config check` checks the file

A revision is put beside the file's bootstrap core, read as the
configuration, resolved, and checked as `ferrofed config check` checks a
file, the transport policy and the registry document included. One the
check refuses is refused with `422 configuration-invalid`, and the message
carries the same error `config check` prints. Nothing is stored, and the
running configuration stays.

## The routes

Every route is on the admin listener and is admitted only for a caller
whose token carries the issuer's `operator_scope`, as the other admin
actions are ([Who the admin listener serves](metrics.md#who-the-admin-listener-serves)).
Each write is recorded under `ferrofed::security` as an admin action, and
with the revision number as `config-revision-saved`,
`config-revision-applied` or `config-rolled-back`.

| Route | Answer |
|---|---|
| `GET /admin/config` | the newest revision's number as `head`, and the running revision's mark as `current`: its number, the one before it, whether it is `applied` or `pending-restart` with the keys a restart applies, who applied it and when; and this replica's own `running`, `in_step` and `refused` ([Several replicas](#several-replicas)) |
| `GET /admin/config/revisions?offset=&limit=` | one page of the history, newest first, at most 100 |
| `GET /admin/config/revisions/{id}` | one revision |
| `GET /admin/config/revisions/{id}/diff/{other}` | each dotted key that differs from `id` to `other`: `added`, `removed` or `changed`, with both values |
| `POST /admin/config/revisions` | saves `{"base": <newest>, "reason": "…", "toml": "…"}`, `201` with the revision |
| `POST /admin/config/revisions/{id}/apply` | applies the newest revision, with no body |
| `POST /admin/config/rollback` | `{"current": <running>, "reason": "…"}`: saves the content of the revision that ran before the running one as a new revision, and applies it |

A save names the revision it is based on, which must be the newest (`null`
for none), and an apply names the newest revision. When another operator,
or another replica on a shared store, saved or applied first, the request
is refused with `409 revision-stale` and nothing changes, so two operators
never overwrite each other. Read the revisions again and base the change
on the newest one.

An apply goes through the same path as a `SIGHUP` reload: the sections a
reload applies take effect at once, and a changed setting that takes a
restart is listed in `needs_restart`, marked `pending-restart`, and takes
effect at the next start. When the reload refuses the revision, the
running configuration and the running mark stay. A rollback is one step:
it restores the revision that ran before the running one, so a second
rollback returns to where you were. With no earlier revision it is refused
with `409 rollback-unavailable`.

The other refusals are `400 body-invalid` for a body that does not read,
whose parser message is never echoed, and `404 revision-unknown`
([Gateway codes](../integrate/errors.md#gateway-codes)).

## Console-managed resources

Each member of the federation, with its organisation and its endpoints, and
each stored-query version has one owner, which `GET /admin/config/resources`
names:

| Owner | Resource | Changed by |
|---|---|---|
| `file` | a member the registry document provides; a stored query of the read-only definition files | the file alone, read-only in the console |
| `directory` | a member the care services directory of `[registry.mcsd]` provides | the directory alone, read-only in the console |
| `console` | a member a revision holds under `[[registry.members]]` | the console, through a revision |
| `gateway` | a stored query of the reserved namespace `eu.ferrofed.eehrxf` | nobody |
| `store` | a stored-query version in the stored-query store | nobody: a held version is immutable (§12.7, N44) |

A member is owned whole. A console member is held in the revisions as one
`[[registry.members]]` entry and joined to the registry document's members,
so a revision stays the whole configuration and nothing is written back to
the file. The configuration file and the environment never set
`registry.members`: a file that does refuses to start. Console members join
a registry document; with a care services directory, which is the whole
registry, they are refused.

```toml
[[registry.members]]
organisation = { id = "org-c", name = "Hospital C" }
node = { id = "node-c", system_id = "cdr-c.example.org", product = "FerroEHR", version = "4.3.3" }
endpoints = [
  { id = "node-c-pub", url = "https://cdr-c.example.org/openehr", connection_type = "openehr-rest-query" },
]
```

| Route | Answer |
|---|---|
| `GET /admin/config/resources` | `running` and `head`, `stored_query_registry` (`absent`, `read-only` or `writable`), every member with its `node`, `organisation`, `system_id`, `endpoints`, `owner`, and for a console member its `state` and the `member` itself, and every stored-query version with its `owner` |
| `PUT /admin/config/members/{node_id}` | `{"base": <newest>, "reason": "…", "member": {…}}`: saves a revision with the member in place of the console member of that `node_id`, or beside the others, `201` with the revision |
| `POST /admin/config/members/{node_id}/remove` | `{"base": <newest>, "reason": "…"}`: saves a revision without that console member, `201` with the revision |
| `PUT /admin/config/stored-queries/{name}/{version}` | `{"aql": "…"}`: stores the version in the stored-query store at once, `201` with it as an ITS-REST `StoredQuery` |

A member save or removal is not applied: preview the revision with
`GET /admin/config/revisions/{running}/diff/{new}`, then apply it, and roll
it back in one step like any revision. A console member's `state` is
`running`, `pending` (saved, not applied), `removing` (the newest revision
removes it), or `shadowed`.

A change that names a member the file or the directory provides, by its
`node_id`, its organisation id, its `system_id`, an `endpoint_id` or an
endpoint's base URL, is refused with `409 resource-file-owned`, and the
removal of a `node_id` no console member has with `404 resource-unknown`.
A stored-query version is admitted as a `PUT` to the ITS-REST definition
API is: a held version is `409 stored-query-held`, the reserved namespace
`409 stored-query-reserved`, and the read-only definition files
`409 resource-file-owned`. A version is never edited or removed, so a
change is a new version.

At a `SIGHUP` reload whose file changed, the file's new revision carries
the console members over from the running revision, so a console member
survives the reload. When the file later provides a member that shares an
identity with a console member, the file's member runs, and the console's
copy stays in the revision, reported as `shadowed` and logged by `node_id`;
remove it, or give it identities of its own. When the console members are
refused beside the file's configuration, as when the file drops the
registry document they join, the file's revision is recorded without them,
the refusal is logged by key, and the revision before it still holds them.

The operator console's Configuration page shows all of this through the
first replica it is configured with: the running and the newest revision
with the newest one's changes, the members and stored queries with their
owners, and the history, with a form, behind a confirmation, to save or
remove a console member, apply the newest revision, roll back, and store a
stored-query version ([The operator console](operator-console.md#configuration)). Its member form
writes `openehr-rest-query` endpoints; a member's `hl7-fhir-rest`
endpoints are saved through the API and kept by the console's saves.

## Several replicas

With `backend = "postgres"`, the replicas share the history and the
running mark, and a save or an apply on one replica is checked against
what every other replica wrote.

Each replica reads the running mark every `config.poll_ms`, 5000 ms by
default. When the mark names a revision the replica does not run, because
another replica applied or rolled back a revision, or recorded its changed
file at a `SIGHUP`, the replica applies that revision through the same
reload path an apply takes. Within one `poll_ms` of an apply, every replica
runs the revision. The watch reads the mark with an ordinary query; it does
not use PostgreSQL `LISTEN`/`NOTIFY`.

A replica can refuse a revision another replica applied, for example when
a file the revision names, such as the registry document, is missing or
invalid on that replica. The replica then logs the refusal, counts it in
`ferrofed_config_revision_follows_total{result="refused"}`, and keeps its
running configuration. It does not try the same revision again. Fix the
cause, then send it `SIGHUP` or apply a newer revision.

`GET /admin/config` on each replica says where that replica stands:

| Member | Meaning |
|---|---|
| `running` | the revision this replica runs, or `null` before the first one |
| `in_step` | whether `running` is the revision the store marks current |
| `refused` | the revision this replica refused, while it does not run it |

`ferrofed_config_revision_in_step` is `1` while the replica runs the
current revision and `0` while it does not, and the alert
`FerroFEDConfigurationDrift` fires after 10 minutes at `0`
([Metrics](metrics.md)).

| Key | Default | What it sets |
|---|---|---|
| `config.poll_ms` | `5000` | how often each replica reads the running mark, from 100 to 600000 ms; read only under `managed`, and a change takes a restart |

Under `config.ui = "off"` nothing is read: no store, no watch.
