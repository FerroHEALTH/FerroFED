<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Stored queries

A deployment that sets `[stored_queries]` offers the federated stored-query
registry, and `OPTIONS {base}/` declares it as
`definition.stored_query_registry: true` (§12.7, N44). The gateway then holds
each definition itself: it is authoritative for it, and a query invoked by
name runs over every member exactly as if you had sent its text to
`POST {base}/v1/query/aql`.

Store a query with its AQL as the `text/plain` body, at a version:

```http
PUT {base}/v1/definition/query/org.example::compositions/1.0.0
Content-Type: text/plain

SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = $patient
  AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'
```

- The name is `[{namespace}::]{query-name}` over `a-z`, `A-Z`, `0-9`, `_`, `.`
  and `-`, and the query name is never `aql` (ITS-REST). The version is
  `major.minor.patch` (ITS-REST's semver path segment, §12.7). Anything else
  is a `400` (`query-name-invalid`, `query-version-invalid`). The body is
  `text/plain`: send that `Content-Type`, with `charset=utf-8` if you like,
  or none. Any other is a `415` (`media-type-unsupported`), and nothing is
  stored. A `PUT` with no
  version is a `400` (`query-version-required`), because the registry stores
  only at a version. `query_type`, when sent, is `AQL` in any case.
- A stored version is immutable. A second `PUT` to a name and version the
  registry holds is a `409` (`stored-query-held`), the held text stands, and
  so does the refusal after the gateway restarts. Store a change as a new
  version (§12.7, N44).
- The gateway analyses the text as it analyses a query you send, with every
  `$parameter` standing in for a value you will bind. A text it would refuse
  whatever you bind is refused now, `400` with the same code a query would
  get (`not-aql`, `unreducible`, and so on). A refusal that depends on how
  you target the query, such as an aggregate across several members, is left
  to each invocation.
- Name the patient through a `$parameter`. A definition that names the
  patient by a literal identifier is refused `400` (`subject-literal`),
  because the registry would hold that identifier at rest (§5.4.1, N33).
  The message never quotes it, and the security log records only the
  position of the predicate.
- The answer is `200` with `Location` naming the stored version, as a
  reference relative to the request URL, because the gateway does not know
  the base URL its clients use (§4.1).
- A deployment may run the registry read-only, its definitions published by
  its operator. Every `PUT` there is a `405` (`stored-query-read-only`) with
  `Allow: GET, OPTIONS`, and `OPTIONS` on the path lists no `PUT`; reading
  and running the definitions it holds work as below.
- Behind several replicas sharing one registry, a version one replica
  stored is read and run at every other, and of two `PUT`s of the same new
  version at once exactly one is stored; the other is a `409`
  (`stored-query-held`).

The gateway holds the canonical print of the parsed query, so a comment or
any other text the parser drops is not kept, and `GET` returns the query in
that form.

Read it back with `GET` on the same path, which answers the ITS-REST
`StoredQuery` (`name`, `type`, `version`, `saved`, `q`), or `404`
(`stored-query-unknown`). `GET {base}/v1/definition/query/{pattern}` lists
every version of every stored query whose name starts with the pattern.

Run it by name, binding its parameters in the ITS-REST `Query` body:

```http
POST {base}/v1/query/org.example::compositions
Content-Type: application/json

{"query_parameters": {"patient": "12345"}}
```

- Without a version, the highest version runs. `/{version}` picks one: an
  exact `major.minor.patch`, or a `{major}` or `{major}.{minor}` prefix that
  runs the highest version it matches (ITS-REST). A name or version the
  registry does not hold is a `404` (`stored-query-unknown`).
- Every rule of an inline query applies unchanged: `offset` and `fetch`, the
  completeness and dedup headers, `Prefer: wait`, and the
  [targeting headers](client-contract.md#pinning-a-query-to-named-systems). A definition that
  carries `FROM ENDPOINT` or `ORGANISATION` is targeted by it, and a header
  that selects other endpoints is a `400` (`targeting-conflict`).
- A parameter you do not bind, or bind and the query does not use, is a `400`
  (`parameters`) naming it, never its value.
- The `Query` body is JSON, under the same `Content-Type` rule as an inline
  query: `application/json` or none, and any other is a `415`
  (`media-type-unsupported`) that asks no node.
- The answer is the ordinary federated `RESULT_SET`, with `name` naming the
  gateway's stored query and `q` its stored text (§9.1, §12.7). No node
  receives your patient identifier, and no node receives the stored query by
  name: each gets the standard AQL of an inline query.
- `GET {base}/v1/query/{name}[/{version}]` runs it the same way with the
  members in the query string, as [the GET form](client-contract.md#the-get-form) of an inline
  query does: `offset` and `fetch` by name, and every other pair a query
  parameter. The stored `GET` forms declare no `q`, so `q=…` binds `$q`.
  `ehr_id` is dropped, and a query string the decoder refuses is a `400`
  (`body-invalid`).

Templates go to the one node you name ([templates and
definitions](templates-and-demographics.md#templates-and-definitions)). Without the registry, `GET` and
`POST {base}/v1/query/{name}` answer `501`.

## Distributing a stored query

A deployment that sets `federation.fan_out_stored_queries` beside the
registry also distributes a definition to the members you name, and
`OPTIONS {base}/` declares `definition.stored_query_fan_out: true` (§12.7,
N44). It is off by default, and it is never declared without the registry.
Where the registry is offered without it, a stored-query `PUT` or a `GET` of
a version that carries `openEHR-federation-endpoint` or
`openEHR-federation-organisation` is a `400`
(`stored-query-fan-out-unsupported`): nothing is stored or read, so a
request for distribution is never answered as a plain one. Where it is
offered:

- Ask for it on the `PUT`: `openEHR-federation-endpoint: *` names every
  active member, and a header that selects endpoints names those. A `PUT`
  that names none is stored at the registry alone, as above. A list naming
  a suspended endpoint, or `*` with no active member, is a `404`
  (`no-destination`) and nothing is stored.
- The registry stores the definition first, under every rule above. Each
  named member is then sent the registry's copy, its canonical AQL, with
  ITS-REST `PUT /definition/query/{name}/{version}` and `query_type=AQL`, on
  its own. No header of yours is sent. A member that fails never removes the
  registry's definition, and a member that accepted is never sent a
  rollback.
- The answer is the registry's `StoredQuery` (`name`, `type`, `version`,
  `saved`, `q`) with `meta.federation` beside it, one `endpoints[]` entry
  per registry member in the shape of a federated result set's (§9.5), and
  `Location` naming the stored version. The statuses are those of the
  [template fan-out](templates-and-demographics.md#fan-out-template-upload): `200` when every member you
  named accepted, `207` with `complete: false` when some did, and `504` or
  `424` when none did. Whatever the status, the registry holds the
  definition. A member that failed carries its HTTP status and an excerpt
  of its message in `error`, as in a query's answer
  ([A node's error in `endpoints[]`](client-contract.md#a-nodes-error-in-endpoints)); no
  other part of a node's body is copied into the answer.
- A definition whose AQL carries a `FROM ENDPOINT` or `ORGANISATION`
  directive is refused `400` (`definition-endpoint-targeted`) and nothing is
  stored: a node cannot run a directive that names members of the
  federation (§12.7, §8.1). Store it without naming members and it runs
  federated, targeted by its directive.
- An invocation always runs the registry's copy, inline, whatever a member
  holds under the same name (§12.7).

```http
PUT {base}/v1/definition/query/org.example::compositions/1.0.0
openEHR-federation-endpoint: *
Content-Type: text/plain

SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = $patient
  AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'
```

A member's copy can drift from the registry's: a failed distribution, a
local `PUT` at the node, a restore, or a member admitted later (§12.7). To
check, `GET` the version naming members in the same headers:

```http
GET {base}/v1/definition/query/org.example::compositions/1.0.0
openEHR-federation-endpoint: *
```

- Each named member is asked for its copy of that version with ITS-REST
  `GET /definition/query/{name}/{version}`. A `{major}` or
  `{major}.{minor}` prefix selects the registry's version first, and the
  members are asked for that one.
- The answer is the registry's `StoredQuery` with `meta.federation`. A member
  whose copy is the same query is `active`; layout and comments do not
  count, because both sides are compared as their canonical prints. A
  member whose copy differs is `node-error` with
  `error.code: "definition-differs"`, and one that holds none is
  `node-error` with `error.code: "definition-missing"`. A member that fails
  or does not answer is reported as in the distribution. No member's copy
  is copied into the answer.
- The status is `200` when every member you named matches, and `207` with
  `complete: false` otherwise. `openEHR-federation-endpoint` and
  `openEHR-federation-system-id` list the matching members.
- Without a header the `GET` answers from the registry alone, as above.
