<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuration

The `ferrofed` binary reads one TOML file and the environment. It serves the
process shape today (health, readiness, the request log, graceful shutdown);
the federation surface under `/v1/` answers `501` until the façade lands.

## Running it

```text
ferrofed serve --config /etc/ferrofed/ferrofed.toml
ferrofed config check --config /etc/ferrofed/ferrofed.toml
```

`--config` names the file; without it the file is the one `FERROFED_CONFIG`
names, and without that every default stands. `config check` reads and
resolves the configuration exactly as `serve` would, secrets included, prints
one line and exits, so a deployment pipeline can test a file without binding a
socket.

A configuration the gateway refuses exits with code 78 (`EX_CONFIG`) and one
line naming the key at fault. It refuses an unknown key, a value of the wrong
type, a zero timeout or body limit, a log filter that does not parse, a secret
set both inline and through its `_file` sibling, and a credentials section
that names no scheme or two. It never falls back to a default for a value you
set.

## The file

```toml
[server]
listen = "127.0.0.1:8080"     # the socket address to bind
request_timeout_ms = 30000    # a request past this answers 408
shutdown_timeout_ms = 10000   # the drain after SIGTERM is bounded by this
body_limit_bytes = 1048576    # a body past this answers 413

[telemetry]
format = "auto"   # auto, json or pretty; auto is json unless stdout is a terminal
filter = "info,hyper=warn,tower=warn,h2=warn"

# Outbound credentials, one section per endpoint id. Each section names one
# scheme: a bearer token, or a user and a password.
[credentials."hospital-a"]
bearer_token_file = "/run/secrets/hospital-a-token"

[credentials."clinic-b"]
user = "ferrofed"
password_file = "/run/secrets/clinic-b-password"
```

## Node selection

A gateway that federates (`registry.document` is set) declares how an
undirected patient query finds its nodes, and refuses to boot without the
declaration:

```toml
[federation]
node_selection = "ask-all"
```

`ask-all` is the selection for a deployment with no localization service (the
specification's reference flow, Variant B; N4). Every active member is a
candidate: the gateway asks every member's cross-reference where the patient
is, dispatches the query only to the members that return an `ehr_id`, and
reports the others as `not-resolved` without failing the query. It is the only
selection the gateway offers until a localizer binding lands; a localizer that
does not answer then fails closed, which is a different rule. The selection is
named in the startup log line.

## Resolution bindings

A query that resolves a patient leaves a binding behind for the client
session: which member holds which `ehr_id`, so a follow-up on a path `ehr_id`
reaches the right node. A binding holds no patient identifier, lives in memory
only, and expires after a lifetime you set:

```toml
[federation]
binding_ttl_ms = 900000   # 15 minutes, the default; 0 is refused
```

The lifetime is a correctness bound. An identity merge or split at the
identity source can make a binding stale, and a binding never outlives its
lifetime, so set it no longer than you would accept a follow-up being routed
on a superseded identity. The gateway also has a hook that drops the affected
bindings the moment a PMIR subscription reports a merge or split. No
subscription is built yet, so the lifetime is the bound in practice; the
specification marks this lifecycle track provisional.

## Completeness

A federated query is all-or-nothing by default (N37). If a node that was
asked does not answer, the query fails: `504` when the node timed out or was
unreachable, and `424` when it answered with an error. When both happen, the
answer is `504`. A failing answer returns no rows, and its `meta.federation`
names every node with its status and `complete: false`. A member that does not
know the patient (`not-resolved`) or that refuses on consent grounds
(`consent-denied`) clears `complete` and never fails the query. A member that
was never in scope (`excluded`, `not-localized`) leaves `complete` alone.

A client can opt into best-effort for one request by sending
`openEHR-federation-completeness: partial`. The gateway then answers `200` with
the rows of the nodes that did answer, still names every other node with its
status, and sets `complete: false`. Sending `all` asks for the default
explicitly. Any other value, or the header given twice, is refused with a
`400`. Best-effort is offered by default, and you can withdraw it:

```toml
[federation]
best_effort = false   # a request asking for partial is then refused with a 400
```

The gateway never quietly serves an all-or-nothing answer to a request that
asked for `partial`. The setting is named in the startup log line.

Every secret has a `_file` sibling, read once at boot and trimmed, so a secret
can come from a mounted file and never sit in the configuration or the
environment. The credentials are read and checked at boot; the node dispatch
hands them to each endpoint once it lands.

## The environment

Any key can be set or overridden with `FERROFED__<SECTION>__<KEY>`, upper or
lower case, with `__` between the segments:

```text
FERROFED__SERVER__LISTEN=0.0.0.0:8080
FERROFED__CREDENTIALS__HOSPITAL_A__BEARER_TOKEN_FILE=/run/secrets/token
```

A value reads as TOML syntax when it is one (`9`, `true`) and as the string it
is otherwise. An override that names no key, or a key the file does not
define, is refused like any other unknown key.

## The HTTP surface

| Route | Answers |
|---|---|
| `GET /` | the product name and version |
| `GET /health` | `200` while the process is up |
| `GET /health/readiness` | `200` when every registered indicator is up, `503` with each indicator's state otherwise |
| any path under `/v1/` | `501` until the façade lands |
| any other path | `404` |

Every response carries an `x-request-id`: the client's value when it is short
printable ASCII, a fresh UUID otherwise.

## What the log records

One line per request: the method, the matched route, the status, the latency
and the request id. A façade query carries the patient identifier, so the line
never carries a request body, the AQL text, a header value, a path no route
matched (it is logged as `<unmatched>`), or a query value other than the
ITS-REST paging parameters `offset` and `fetch`, and those only when they are
digits. A handler panic answers `500` and is logged without its message, which
could quote a value the handler held.
