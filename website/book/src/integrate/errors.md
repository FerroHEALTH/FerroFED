<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Errors and status codes

When FerroFED fails a request, the HTTP status follows the table of §11.2 of
the Federation Tier specification, and the body names a stable code your
client can branch on. This page lists every code.

The codes are API. A code is only ever added: it is never renamed, never
removed and never moved to another status, so a client written against this
page keeps working across releases. A test holds this page to the gateway's
own table, so a code the gateway answers is always listed here.

## The error body

A failure the gateway reports on its own behalf answers with the openEHR
ITS-REST `Error` body and two more members:

- `message`: a sentence for a person. Its wording can change between
  releases, so do not parse it.
- `validationErrors`: the ITS-REST list, which is empty.
- `code`: the stable code, from the tables below.
- `request_id`: the request id, the value of the `X-Request-Id` response
  header. It is your own `X-Request-Id` when you sent one, and otherwise the
  gateway's own id, the one its log and every node record. Your own id never
  leaves the gateway: no node receives it and the log does not record it,
  because the gateway cannot tell whether free text names a patient (§5.4.1,
  N33). To name a request the operator can find by its id, send none.

```json
{
  "message": "offset-based paging is not supported across a fan-out (§11.6.2, N39)",
  "validationErrors": [],
  "code": "offset-unsupported",
  "request_id": "6f1c0b9e-3d1a-4c55-9a43-0d8f1f2b7c11"
}
```

No error body quotes your request: not the AQL text, not the value of a
query parameter, not a header value and not the path. A message about part of
the query points at it by byte range (`bytes 52..77`), and a message about a
query parameter names the parameter and never its value (§5.4.3). This holds
for every answer in the code tables below. A failed fan-out answers a result
set instead, which echoes your own query as `q`, as every result set does
(N17).

## A failed fan-out

Under the default all-or-nothing strategy, a query fails when a node it asked
did not answer or answered with an error (§11.4, N37). That answer is not an
error body. It is the federated `RESULT_SET` with your `q`, your `columns`
and no rows (N17), and its `meta.federation` carries `complete: false` and
every node with its status (§11.1, §11.4):

| Status | When | The cause, in `meta.federation.endpoints[]` |
|---|---|---|
| 504 | a node did not answer in time, or could not be reached | `time-out` or `offline`, with the node's `error` |
| 424 | a node answered with an error, or with a result the gateway cannot use (under `version-identity`, a version uid that is not an `OBJECT_VERSION_ID`) | `node-error`, with the node's own failure in `error` |
| 424 | the cross-reference service could not answer for a member | `not-resolved`, with the service's failure in `error` |

When both a 504 and a 424 cause occur, the answer is `504` (§11.4). With
`openEHR-federation-completeness: partial` on a gateway that offers it, the
same causes answer `200` with the rows of the nodes that did answer, and the
failed nodes reported the same way.

Two statuses are answers and never fail a query: `not-resolved` (the patient
is not known at that node) and `consent-denied`. A query where no node knows
the patient answers `200` with no rows (§11.3).

## A node's own answer

On a route the gateway forwards to one node, the node's status and body pass
through as the node sent them: a node's `404` is the node's `404`, and a
node's `500` is the node's `500` (§11.2). Those answers carry no FerroFED
code, because the body is the node's. The single-node routes are the EHR
resources under a path `ehr_id`, `{base}/v1/ehr/{ehr_id}` and below it
(§7a.1). Every other ITS-REST path except the federated query answers `501`
with the code `not-implemented`.

Three answers on a routed request are the gateway's, because the node gave
none of its own to pass on: `node-timeout` and `node-unreachable` (`504`),
and `node-refused` (`424`) when the node refused the gateway's onward
credentials with a `401`. That `401` is about the gateway's credentials, not
yours, so it is never passed to you as a challenge. Every answer to a routed
request, these three included, names the acting endpoint in
`openEHR-federation-endpoint` and its node's `system_id` in
`openEHR-federation-system-id` (N31, §9.6).

Inside a fan-out, a node's `404` or `500` is not passed through. The node is
reported `node-error`, and the query fails with `424`, or, under `partial`,
the node is reported and the query succeeds.

## Gateway codes

| Code | Status | When |
|---|---|---|
| `body-invalid` | 400 | The request body is not the ITS-REST request the route takes, for example an `AdhocQueryExecute` without a string `q`. |
| `completeness-invalid` | 400 | The `openEHR-federation-completeness` header is repeated, or carries neither `all` nor `partial` (§11.4). |
| `partial-unsupported` | 400 | The request asks for `partial`, and this gateway does not offer best-effort (§11.4, N37). |
| `dedup-invalid` | 400 | The `openEHR-federation-dedup` header is repeated, or names neither `none` nor `version-identity` (§10, §7a.2). |
| `parameter-invalid` | 400 | A query parameter is `null`, an array, an object, or an integer outside 64 bits. |
| `patient-invalid` | 400 | The query's patient identifier or namespace cannot form a patient reference (§5.2). |
| `no-destination` | 404 | The request can be routed to no destination at all: node selection left no registry member in scope, or the `ORGANISATION` directive names only organisations that manage no endpoint (§11.2, §11.3). |
| `ehr-id-collision` | 409 | The `ehr_id` is claimed by more than one node; the gateway never chooses between them (§12.5.2, N42). |
| `controlling-system-unreachable` | 409 | A versioned write's controlling system is not reachable, and the gateway never writes to a copy (§10.3, N36). |
| `internal` | 500 | The gateway failed on its own side. The operator's log records the failure under the gateway's request id. |
| `not-found` | 404 | The path is outside every surface the gateway serves. |
| `not-implemented` | 501 | The path is an ITS-REST area the gateway does not expose (§7a.1, N32), or the query selects ENDPOINT attributes through the `FROM ENDPOINT` variable (`p/id`, `p/system_id`), which the gateway does not add to rows; that is planned build order (§9.3, N12). A read of an EHR resource that names no node in `openEHR-federation-endpoint` answers it too, until the gateway can find the node by itself (§12.5.1). |
| `endpoint-unknown` | 400 | The `FROM ENDPOINT` directive names an identifier that is not an endpoint of the registry (§8.4.1, N19). The message points at the identifier by its place in the list and never quotes it. The `openEHR-federation-endpoint` header of a request routed to one node answers it too when it names an endpoint the registry does not hold (§8.4.1). |
| `organisation-unknown` | 400 | The `ORGANISATION` directive names an identifier that is not an organisation of the registry (§8.1, §8.4.1, N20). The message points at the identifier by its place in the list and never quotes it. |
| `target-required` | 400 | A write to an EHR resource names no node in `openEHR-federation-endpoint`, and nothing else routes it; the gateway never finds a write's destination by trial (§12.5.1, N41). |
| `endpoint-several` | 400 | A request routed to one node names more than one endpoint in `openEHR-federation-endpoint` (§7a.1, §12.4). |
| `query-parameter-refused` | 400 | A request routed to one node carries a query parameter ITS-REST does not define for the EHR resources. The gateway cannot tell an identifying value from any other, so it sends nothing; the message names the parameter by position, never by name or value (§5.4.1, N33). |
| `node-timeout` | 504 | The node a request was routed to did not answer in time (§11.2). |
| `node-unreachable` | 504 | The node a request was routed to could not be reached (§11.2). |
| `node-refused` | 424 | The node a request was routed to refused the gateway's onward credentials (§11.2). |

The two `409` codes belong to follow-up routing (§12), which is planned build
order; the codes are fixed now, so a client can handle them before they
occur.

## Query refusals

The gateway refuses a query it cannot answer correctly before it sends
anything to a node (§5.4.1, §7.1, §11.6). Every refusal is a `400`.

| Code | Status | When |
|---|---|---|
| `not-aql` | 400 | The query is not AQL 1.1.0. |
| `parameters` | 400 | The query parameters cannot be bound: one is used without a value, supplied but unused, supplied twice, or of a form its position cannot hold. |
| `unreducible` | 400 | The patient predicate cannot be reduced to one `ehr_id` scope per node: it sits under `OR` or `NOT`, is not an `=`, compares with a non-literal, or uses another subject path (§7.1, §5.4.3). |
| `identifier-not-string` | 400 | The patient identifier or its namespace is compared with something other than a string. |
| `second-subject` | 400 | The query names two different patient identifiers (§7.1). |
| `second-namespace` | 400 | The query names two different issuing namespaces for the patient (§5.2, §7.1). |
| `empty-identifier` | 400 | The patient identifier is empty (§5.2). |
| `no-namespace` | 400 | The patient identifier carries no namespace, and the deployment configures no default (§5.2). |
| `subject-projection` | 400 | A selected subject column is not one the gateway can fill in from the resolution input (N5, §7.1). |
| `subject-without-predicate` | 400 | The subject column is selected, and the query names no patient (N5). |
| `subject-ordering` | 400 | A subject path appears in `ORDER BY`, which would carry it to a node (§5.4.2). |
| `identifier-elsewhere` | 400 | The patient identifier appears in another position of the query, where it would reach a node (§5.4.1, N33). |
| `unfoldable-function` | 400 | A string function over a literal is compared with an identifier path and cannot be folded at the gateway (§5.4.1). |
| `undirected-aggregate` | 400 | An aggregate cannot be computed correctly across nodes; direct the query to one node, or select the rows and aggregate them (N14, §11.6.3). The function is not one the gateway declares decomposable. |
| `indecomposable-aggregate` | 400 | A declared decomposable aggregate cannot be recombined exactly in this query: it selects `DISTINCT`, uses `COUNT(DISTINCT …)`, selects a column that is not an aggregate beside it, or the request selects `openEHR-federation-dedup: version-identity`; direct the query to one node, or select the rows and aggregate them (N14, §11.6.3). |
| `partial-aggregate` | 400 | The request asks for `partial`, and the query is an aggregate recombined across nodes, which is exactly correct only over every node (§11.6.3, §11.4). |
| `undefined-function` | 400 | The query calls a function AQL 1.1.0 does not define, such as a product-specific `MEDIAN`, and would reach more than one node. The gateway cannot tell whether the function aggregates; direct the query to one node, or select the rows and compute it in the application (N14, §11.6.3). The functions AQL defines (`LENGTH`, `ROUND`, `NOW`, `TERMINOLOGY` and the rest) are sent to every node as written. |
| `offset-unsupported` | 400 | Offset-based paging is not supported across a fan-out (§11.6.2, N39). |
| `offset-page` | 400 | The gateway computes an `OFFSET` page from `k + n` rows per node, and this page cannot be computed that way: `k + n` is past the configured bound, or the query has no `LIMIT` or no `ORDER BY` (§11.6.2, N39). |
| `paging-conflict` | 400 | The ITS-REST `offset` or `fetch` member and the query's `OFFSET` or `LIMIT` disagree. |
| `negative-paging` | 400 | An ITS-REST paging member, or a row count of the query, is negative. |
| `top-backward` | 400 | `TOP … BACKWARD` is not supported across a fan-out; write `ORDER BY … DESC LIMIT n`. |
| `top-with-limit` | 400 | The query uses the deprecated `TOP` together with a `LIMIT` clause, which AQL forbids; write `ORDER BY … LIMIT n`. |
| `top-with-fetch` | 400 | The query uses `TOP` and the request carries the ITS-REST `fetch` member, which cannot be combined with it; write `ORDER BY … LIMIT n`, or send `fetch` alone. |
| `order-not-selected` | 400 | Under `DISTINCT`, an `ORDER BY` path is not also selected (N13). |
| `unordered-distinct-cut` | 400 | Under `DISTINCT` with `ORDER BY` and `LIMIT`, a selected function column reads a path that is not selected, or a value from outside the row (`NOW()` and the other clock functions, `TERMINOLOGY`). AQL orders a node only on paths, so a node cut at its `LIMIT` could keep different rows on each repeat (§11.6.1, AQL §ORDER BY). Select the paths the function reads, or drop the `LIMIT`. |
| `node-set-undefined` | 400 | The query names no patient and no endpoints, so no node set is defined (N4, §8). |
| `endpoint-variable` | 400 | The variable of the `FROM ENDPOINT` directive is bound again in `FROM`, or used anywhere but as a selected column: in `WHERE`, in `ORDER BY` or inside a function. A path through it selects an ENDPOINT attribute (§8.1, §9.3). |

## Other answers

Two answers come from the HTTP layer around the gateway and carry no error
body or code: `408`, with an empty body, when a request runs past the
server's request timeout, and `413`, with a short plain-text body, when the
request body is over the configured size limit.
