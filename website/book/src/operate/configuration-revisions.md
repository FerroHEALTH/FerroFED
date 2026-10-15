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
(#576). The operator console does not edit the configuration yet; editing
members, endpoints and stored queries there is planned (#577), then
credentials and bindings with a confirmation step and a second operator's
approval (#578), then export to and import from TOML (#579).

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

Under `off`, any other key of `[config]` refuses to start, so a store you
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
| `GET /admin/config` | the newest revision's number as `head`, and the running revision's mark as `current`: its number, the one before it, whether it is `applied` or `pending-restart` with the keys a restart applies, who applied it and when |
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

## Several replicas

With `backend = "postgres"`, the replicas share the history and the
running mark, and a save or an apply on one replica is checked against
what every other replica wrote. An apply takes effect on the replica that
served it; each other replica runs it at its next `SIGHUP` reload or
restart, so send `SIGHUP` to each replica after an apply, as you do after
a file change.
