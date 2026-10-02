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
