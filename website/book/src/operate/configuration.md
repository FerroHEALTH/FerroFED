<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuration

The `ferrofed` binary reads one TOML file and the environment. It serves the
process shape (health, readiness, the request log, graceful shutdown) and, once
a registry is configured, the federated query `POST /v1/query/aql`; every other
path under `/v1/` answers `501`.

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
set both inline and through its `_file` sibling, a credentials section
that names no scheme or two, and a credential the `Authorization` header
cannot carry: a bearer token holding a control character such as a newline,
or a basic user or password holding one (RFC 7617 §2), or a basic user
holding a colon. That refusal names the key the value came from, and never
the value. The gateway never falls back to a default for a value you set.

## The file

```toml
[server]
listen = "127.0.0.1:8080"     # the socket address to bind
request_timeout_ms = 30000    # a request past this answers 408; see Timeouts
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

Every secret has a `_file` sibling, read once at boot and trimmed, so a secret
can come from a mounted file and never sit in the configuration or the
environment. The credentials are read and checked at boot, and the node
client of an endpoint with a credentials section sends them on every request
to that endpoint.

## The registry document

`registry.document` names a second TOML file: the federation's members as
the operator admitted them. It declares each `[[organisation]]`, each
`[[node]]` with its openEHR `system_id`, and each `[[endpoint]]` with its base
URL, connection type and managing organisation. An unknown key, a dangling
reference or a duplicate id refuses the whole document.

The document is also the follow-up routing table (N21). A follow-up for a
version is routed on the `creating_system_id` inside its uid (§12.2). A
member's own `system_id` routes to that member without being written down.
A CDR can hold versions another system created, because an imported
composition keeps its original uid. Map every other `creating_system_id` you
know of to the endpoint that answers for it with a `[[creating_system]]`
entry:

```toml
[[creating_system]]
creating_system_id = "legacy-a.example.org"   # the middle segment of the uid
endpoint = "hospital-a"                       # an endpoint id this document declares
```

`config check` refuses, naming the `creating_system_id`, a mapping that names
an endpoint the document does not declare, a `creating_system_id` mapped
twice, and a mapping of a member's own `system_id`. Two spellings that differ
only in ASCII case are one `creating_system_id`.

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
was never in scope (`excluded`, `not-localized`) leaves `complete` alone. When
every endpoint is `excluded`, for example because every one is suspended, no
member is in scope and the request cannot be resolved to any destination: the
gateway answers `404` and asks no node (§11.2, §11.3).

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

## Timeouts

A federated query runs under two budgets (§11.5, N38): each node's request may
take `per_node_timeout_ms`, and the whole fan-out, resolution included, may
take `overall_timeout_ms`. A node past either is abandoned and reported
`time-out`, so under all-or-nothing the query fails `504` with
`meta.federation` naming every node.

```toml
[server]
request_timeout_ms = 30000    # must exceed overall_timeout_ms by more than 1000

[federation]
per_node_timeout_ms = 10000   # one node's request
overall_timeout_ms = 25000    # the whole fan-out
```

The server's own `request_timeout_ms` answers `408` with an empty body, which
would drop that envelope. A gateway that federates therefore refuses to boot,
and `config check` refuses the file, unless `server.request_timeout_ms` is
greater than `federation.overall_timeout_ms` plus one second, the time the
gateway keeps for combining the answers. The refusal names both keys. The
defaults leave four seconds to spare. The one second is FerroFED's own choice:
§11.5 promises an answer "within its declared overall budget, plus combining
time" and does not size the combining time.

## Paging with `OFFSET`

A node's rows `k` to `k + n` are not the federation's rows `k` to `k + n`, so
the gateway never sends `OFFSET` to a node (§11.6.2, N39). By default it
computes the page exactly: for `ORDER BY … LIMIT n OFFSET k` it asks each node
for its first `k + n` rows with no `OFFSET`, merges them in the federation
order, and returns rows `k` to `k + n`. A node that returns its `k + n` rows
out of that order is reported `node-error`, as for `LIMIT n`. The ITS-REST
`offset` and `fetch` members page the same way.

The page is computed only where `k + n` is bounded. The gateway answers `400`
for a page whose `k + n` is past `max_offset_window` (the message names the
bound), for an `OFFSET` with no `LIMIT`, and for an `OFFSET` with no
`ORDER BY`, which has no order to page through. You can lower or raise the
bound, or refuse every `OFFSET` past zero:

```toml
[federation]
offset_strategy = "bounded"   # the default; "reject" answers every OFFSET > 0 with a 400
max_offset_window = 1000      # rows asked of one node for a page, k + n; 0 is refused
```

The strategy and the bound are named in the startup log line.

## Aggregates across nodes

Each node answers an aggregate with its own value, so one row per node is
never the federation's answer (§11.6.3, N14). By default the gateway
recombines the aggregate exactly: it sends the aggregate to every node, scoped
to that node's `ehr_id`, and answers one row in your query's columns.

- `COUNT(*)` and `COUNT(path)` are the sum of the node counts.
- `SUM` is the sum of the node sums, or `NULL` when no node holds a value.
- `MIN` and `MAX` are the least or greatest node value, over numbers and
  complete date-times.
- `AVG` is asked of each node as the `SUM` and the `COUNT` of its path, and
  answered as their quotient, or `NULL` when no node counts a value.

Integers add exactly, and reals add in decimal arithmetic, so `0.1 + 0.2` is
`0.3`. A recombination over some of the nodes would be a wrong value, so:

- a node that answers a value the recombination cannot use exactly (a string
  for `MIN`, a real for `COUNT`, more or less than one row) is reported
  `node-error`, and the query fails `424`;
- a node that does not answer fails the query `504`, as for any query;
- a request with `openEHR-federation-completeness: partial` is refused `400`
  with the code `partial-aggregate`.

`COUNT(DISTINCT …)`, `SELECT DISTINCT`, and a column that is not an
aggregate beside the aggregates are refused `400`
(`indecomposable-aggregate`), and the message suggests the two alternatives:
direct the query to one node, or select the rows and aggregate them in your
application. A query directed to one endpoint is sent to it unchanged.

You can narrow the functions the gateway recombines, or turn recombination
off, which refuses every undirected aggregate with `undirected-aggregate`:

```toml
[federation]
decomposable_aggregates = ["COUNT", "SUM", "MIN", "MAX", "AVG"]   # the default; [] declares none
```

The list is named in the startup log line.

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
| `POST /v1/query/aql` | the federated `RESULT_SET`; `501` when no registry is configured |
| any other path under `/v1/` | `501` |
| any other path | `404` |

Every response carries an `x-request-id`: the client's value when it is short
printable ASCII, the gateway's own id otherwise.

## Request ids

The gateway mints its own id, a fresh version 4 UUID, for every request. That
id is the `X-Request-Id` of every request the gateway sends to a node for it,
the same id on every node of one fan-out. The client's `x-request-id` never
reaches a node: it is free text, and the gateway cannot tell whether it names
a patient (§5.4.1, N33). When the client sends no id, the response carries the
gateway's id, so the client, the log and every node name the same request.
The outbound gate searches every other part of a node request for the
identifiers resolution consumed, and skips the minted id: it holds no client
input, and a short hexadecimal identifier can occur inside a random UUID.

Every other header the gateway sends to a node is fixed by the gateway:
`Accept` and `Content-Type` (`application/json`), `Authorization` (the
endpoint's configured onward credential, when it has one), and the `Host`,
`Content-Length` and `Accept-Encoding` the HTTP client writes. None is copied
from the client request.

## What the log records

One line per request: the method, the matched route, the status, the latency,
the gateway's request id (`request_id`) and whether the client sent its own
(`client_named`). A façade query carries the patient identifier, so the line
never carries a request body, the AQL text, a header value, a path no route
matched (it is logged as `<unmatched>`), or a query value other than the
ITS-REST paging parameters `offset` and `fetch`, and those only when they are
digits. The client's own `x-request-id` is a header value too, and it is never
logged, so a request the client named is found in the log by its time, route
and status, and in a node's log by the `request_id` of that line. A request
the gateway answers before its handler finishes has its line too, with the
status it answered: `500` for a handler panic, `408` past the request
timeout, `413` over the body ceiling. A handler panic is also logged under
the gateway's id, without its message, which could quote a value the handler
held. A federated query the gateway fails with a `500` also logs "the
federated query failed" with its error code and the same `request_id` as its
request line.
