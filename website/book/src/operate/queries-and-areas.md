<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Queries and API areas

The settings a federated query runs under (completeness, timeouts, paging and
aggregates), the stored-query registry, the DEMOGRAPHIC area and the template
upload fan-out.

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
  answered as their quotient, or `NULL` when no node counts a value. The type
  of the input decides the type of the answer (AQL 1.1.0 §3.9.1.5), and it
  reaches the gateway as the type of the node sums. Over integers, `AVG` is
  an integer: the one nearest the exact quotient of the federation's sum and
  count, with a tie going to the even integer, so `5 / 2` is `2` and `7 / 2`
  is `4`. The gateway rounds once, after it adds every node's sum and count,
  and never rounds a node's own mean. Over reals, `AVG` is the decimal
  quotient written as the nearest JSON number, so `12.5 / 3` is
  `4.166666666666667`. AQL states no rounding rule, so the rounding is
  FerroFED's own.

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

## Stored queries

The gateway can hold stored queries itself, the federated stored-query
registry of §12.7 (N44). Name the file it keeps them in, and the registry is
offered:

```toml
[stored_queries]
path = "/var/lib/ferrofed/stored-queries.redb"
```

- The file is an embedded `redb` store, created at boot when it does not
  exist. Put it on a persistent volume: a stored version must outlive the
  process, because clients invoke it by name after a restart, and a second
  `PUT` refused before a restart is refused after it too.
- One gateway process opens the file at a time. A second process pointed at
  the same file refuses to start, so run one gateway per file.
- The file holds each definition's name, version, the instant it was stored
  and its AQL, and nothing else. A definition names its patient through a
  `$parameter`, never a literal, and the values a client binds when it
  invokes a query are never written, so no patient identifier reaches the
  file (§5.4.1, N33).
- The registry needs the federation that runs its queries, so setting
  `stored_queries.path` without `registry.document` refuses the
  configuration, naming `registry.document`.
- `OPTIONS {base}/` declares `definition.stored_query_registry: true` while
  the path is set, and `false` without it; without it a stored-query
  definition request goes to the one node the targeting headers name, both
  `PUT`s included, and `GET` and `POST /v1/query/{name}` answer `501`.
  Whether the registry is offered is named in the startup log line.

The [client contract](../integrate/stored-queries.md) says how
a client stores and invokes a query.

### Distributing stored queries to the members

The registry runs its own copy of every definition, so no member needs one.
A deployment that wants each definition at the members too, for a node that
runs it by name locally or for an audit at the point of execution, can have
the registry distribute it (§12.7):

```toml
[federation]
fan_out_stored_queries = true   # off by default

[stored_queries]
path = "/var/lib/ferrofed/stored-queries.redb"
```

- Distribution is a facility of the registry. Setting
  `fan_out_stored_queries` without `stored_queries.path` refuses the
  configuration, at `config check` and at start.
- Only a stored-query `PUT` that asks for it is distributed, with
  `openEHR-federation-endpoint: *` (every active member) or headers that
  name members. The registry stores the definition first; each named member
  is then sent the registry's copy independently, within the request's
  budget, and nothing is rolled back. A `PUT` naming no member stores at the
  registry alone. With the setting off, a stored-query `PUT` or version
  `GET` that carries a targeting header is refused `400`
  (`stored-query-fan-out-unsupported`), so a client that asked for
  distribution never mistakes a plain store for it.
- A definition that carries a `FROM ENDPOINT` or `ORGANISATION` directive is
  refused for distribution, because no node can run it. Store it without the
  header, and it runs federated.
- A `GET` of a stored version that names members reports, per member,
  whether its copy matches the registry's. An invocation always runs the
  registry's copy, never a member's.
- `OPTIONS {base}/` declares the setting as `definition.stored_query_fan_out`,
  `true` only while the registry is offered. The setting is named in the
  startup log line, and changing it needs a restart.

The [client
contract](../integrate/stored-queries.md#distributing-a-stored-query) gives
the answers.

## The DEMOGRAPHIC area

The gateway never federates the openEHR DEMOGRAPHIC API (§7a.1, N32): the
patient's identity is resolved through the identity binding, never through a
node's demographic store. By default every request under `/v1/demographic/`
answers `501` and no node is asked.

A deployment that keeps its demographics in one member may declare that
member's endpoint as the one that serves the area:

```toml
[federation]
demographic_endpoint = "hospital-a.demographic"
```

- The request chooses its node, as a definition request does (§7a.1, §12.4,
  §12.6, N23): every DEMOGRAPHIC operation ITS-REST 1.1.0 defines names that
  endpoint in `openEHR-federation-endpoint`, and then goes to it alone,
  through the same single-node path as a definition request:
  the query string and the declared headers are held to what the operation
  declares, the body travels byte-identical, and the node's answer comes back
  as the node sent it, with `openEHR-federation-endpoint` and
  `openEHR-federation-system-id` naming the endpoint (§7a.3, N31). Nothing is
  probed, fanned out or sent to another member.
- The gateway never applies the setting as a default. A request naming no
  endpoint is a `400` (`target-required`), and no node is asked. A header
  naming another endpoint is a `400` (`targeting-conflict`), several
  endpoints an `endpoint-several`, and `*` or an unknown id an
  `endpoint-unknown`.
- The value must be an endpoint id of the registry. Any other value refuses
  the configuration, naming `federation.demographic_endpoint`, and so does
  setting it without `registry.document`. A suspended endpoint answers
  `no-destination`.
- `OPTIONS {base}/` declares `its_rest.demographic` as `unsupported: 501`
  without the setting, and as `routed-single-node` naming the endpoint a
  client names, with it. The setting is named in the startup log line.

## Fan-out template upload

A template lives at the node it was uploaded to, so a deployment whose
clients commit to any member needs the same template at every member
(§12.6). By default the gateway routes every template upload to the one
endpoint the request names, and the deployment keeps templates consistent
another way: distributing them out of band, or through a shared template
repository (§12.6, N43). The gateway can instead fan an upload out:

```toml
[federation]
fan_out_template_upload = true   # off by default
```

- Only an ADL 1.4 or ADL 2 template upload fans out, and only when the
  request asks for it, with `openEHR-federation-endpoint: *` (every active
  member) or headers that select several endpoints. A plain upload still
  names its one node, and every other definition request still routes to
  one node.
- Each member is sent the upload independently, within the request's
  `per_node_timeout_ms` and `overall_timeout_ms`. A member that accepts keeps
  the template: the gateway rolls nothing back.
- The answer names each member's outcome, and a partial success answers
  `207`, never `200` ([client
  contract](../integrate/templates-and-demographics.md#fan-out-template-upload)).
- `OPTIONS {base}/` declares the setting as
  `definition.fan_out_template_upload`, and `its_rest.definition` says that
  an upload naming `*` or several endpoints fans out. The setting is named
  in the startup log line, and changing it needs a restart.

