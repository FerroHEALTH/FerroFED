and the gateway never picks one for you (§12.6, N43). So does a DEMOGRAPHIC request where the deployment declared the area's endpoint: the request names that endpoint, and the declaration is never applied as a default (§7a.1, §12.4, §12.6, N23). |<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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
for every answer in the code tables below. A `targeting-conflict` message
names the node sets your request selected, as §8.4.1 requires, by the
registry's own endpoint identifiers, which are membership information and
carry no patient data (§7a.2). A failed fan-out answers a result
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
(§7a.1), `POST {base}/v1/ehr`, the creation of an EHR (§12.4),
`GET {base}/v1/ehr?subject_id=…&subject_namespace=…`, the read of an EHR by
subject, which the gateway sends as `GET {base}/v1/ehr/{ehr_id}` to the one
member that resolves the subject, and every
request under `{base}/v1/definition/` the stored-query registry does not
answer itself (§12.6). A template-missing
validation failure a node reports is that node's error and passes through
unmasked (§12.6). A request under `{base}/v1/demographic/` answers `501`
unless the deployment declared its endpoint; then it is routed when it names
that endpoint, and is `target-required` without the header (§7a.1, §12.6,
N32). Every other ITS-REST path except the federated query and,
where the registry is offered, a stored query run by name answers `501` with
the code `not-implemented`. On a request routed by its target alone (a new
EHR, a definition request or a DEMOGRAPHIC request), a malformed declared
value is refused before the missing target is: `parameter-value-invalid`, or
the `406` or `415`, comes before `target-required`.

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

## A fan-out template upload

Where the deployment offers fan-out template upload, an upload naming `*` or
several endpoints is answered per node, and no node's body passes through
(§12.6, N43). The body is `meta.federation` alone, one `endpoints[]` entry per
registry member, as in a federated result set (§9.5):

| Status | When |
|---|---|
| 200 | every member the request named accepted the template, so `complete` is `true` |
| 207 | some members accepted and others failed: `complete` is `false`, each failed member is `node-error`, `time-out` or `offline` with its `error`, and the members that accepted keep the template (§12.6) |
| 424 | no member accepted, and every one answered with a failure (`node-error`, the node's HTTP status in `error`) |
| 504 | no member accepted, and at least one timed out or could not be reached (§11.2) |

The provenance headers name only the members that accepted (§7a.3).

## A distributed stored query

Where the deployment distributes stored-query definitions, a `PUT` naming
members is answered on the statuses of the table above, and the definition is
stored at the registry first whatever they are (§12.7, N44). The body is the
registry's `StoredQuery` with `meta.federation` beside it, so even a `424` or
a `504` names the definition the registry holds.

A `GET` of a version naming members reports drift: `200` when every named
member's copy matches, and `207` otherwise. A member whose copy differs, or
that holds none, is `node-error` with an `error` object whose `code` is
`definition-differs` or `definition-missing`. §11.1 has no status for drift,
and its set is closed, so the code says which.

## Gateway codes

| Code | Status | When |
|---|---|---|
| `body-invalid` | 400 | The request body is not the ITS-REST request the route takes, for example an `AdhocQueryExecute` without a string `q`. On a `GET` form of query execution, and on a stored-query `PUT`, it answers a query string the ITS-REST decoder refuses: a required parameter absent, a parameter given twice, a value that is not of its declared type, or a pair that does not percent-decode to UTF-8 text. |
| `completeness-invalid` | 400 | The `openEHR-federation-completeness` header is repeated, or carries neither `all` nor `partial` (§11.4). |
| `partial-unsupported` | 400 | The request asks for `partial`, and this gateway does not offer best-effort (§11.4, N37). |
| `dedup-invalid` | 400 | The `openEHR-federation-dedup` header is repeated, or names neither `none` nor `version-identity` (§10, §7a.2). |
| `parameter-invalid` | 400 | A query parameter is `null`, an array, an object, or an integer outside 64 bits. |
| `patient-invalid` | 400 | The query's patient identifier or namespace cannot form a patient reference (§5.2). It also answers `GET {base}/v1/ehr` when `subject_id` or `subject_namespace` is absent, empty or given more than once (ITS-REST 1.1.0 declares both required). |
| `no-destination` | 404 | The request can be routed to no destination at all: node selection left no registry member in scope, or the `ORGANISATION` directive or the `openEHR-federation-organisation` header names only organisations that manage no endpoint (§11.2, §11.3). It also answers a read of an EHR resource that nothing routes, and a query scoped to one `ehr_id`, when every member the ask-all probe asked answered `404` (§12.5.1), and `GET {base}/v1/ehr` when no member, or not the member the `openEHR-federation-endpoint` header names, holds an EHR for the subject. That is the operation's own `404` for a subject with no EHR: it reads one EHR resource, so the `200` with no rows of §11.3, which answers a query, does not apply (ITS-REST 1.1.0, §11.2). |
| `ehr-id-collision` | 409 | The `ehr_id` is claimed by more than one node: your session's resolution bindings or the gateway's `ehr_id` index hold it at two members or more, or the ask-all probe of a read, or of a query scoped to that `ehr_id`, found it at two members or more. The message lists the claiming endpoints. The gateway never chooses between them and sends the request, a read or a write, to neither; name the node in the `openEHR-federation-endpoint` header to address one of them (§12.5.1, §12.5.2, N41, N42). |
| `controlling-system-unreachable` | 409 | A versioned write (an update of a composition, `EHR_STATUS` or directory, a delete, or a `CONTRIBUTION` amending versions) amends a version its path `ehr_id`'s node did not create: the registry routes that preceding version's `creating_system_id` to another member, or to none. The write goes to its controlling CDR alone, and the path `ehr_id` belongs to the other node, so the gateway sends it to no node, the controlling node included, whether that node is up or down. A write against a row that de-duplication kept is held to the same rule. The message names the controlling system: the version's `creating_system_id` as the registry spells it, the controlling node, and its endpoint. For a system the registry does not know, it points at where your request names the version (`If-Match`, the path, or a `CONTRIBUTION` version by position) and never quotes your request (§10.3, §12.4, §12a.1, N23, N36). |
| `internal` | 500 | The gateway failed on its own side. The operator's log records the failure under the gateway's request id. |
| `not-found` | 404 | The path is outside every surface the gateway serves. |
| `not-implemented` | 501 | The path is an ITS-REST area the gateway does not expose (§7a.1, N32): the DEMOGRAPHIC API, unless the deployment declares the one endpoint that serves it, and every area not built yet. |
| `endpoint-unknown` | 400 | The `FROM ENDPOINT` directive or the `openEHR-federation-endpoint` header names an identifier that is not an endpoint of the registry, or the header names no identifier at all (§8.4.1, N19). This holds on every request the header applies to: a query, and a request routed to one node. The message names the directive or the header, points at the identifier by its place in the list, and never quotes it. |
| `organisation-unknown` | 400 | The `ORGANISATION` directive or the `openEHR-federation-organisation` header names an identifier that is not an organisation of the registry, or the header names no identifier at all (§8.1, §8.4.1, N20). The message points at the identifier by its place in the list and never quotes it. |
| `target-required` | 400 | A write to an EHR resource names no node in `openEHR-federation-endpoint`, and neither a resolution binding of the session nor the gateway's `ehr_id` index names exactly one; the gateway never finds a write's destination by trial, so nothing is probed (§12.5.1, N41). The creation of an EHR, `POST {base}/v1/ehr` or `PUT {base}/v1/ehr/{ehr_id}`, always names its node in the header, because a new EHR has no owner for a binding or the index to name (§12.4, N23). So does every request under `{base}/v1/definition/` the stored-query registry does not answer: a template lives at the node it was sent to, and the gateway never picks one for you (§12.6, N43). |
| `endpoint-several` | 400 | A request routed to one node selects more than one endpoint through `openEHR-federation-endpoint` or `openEHR-federation-organisation` (§7a.1, §12.4). A definition request is never fanned out, so a `*` there is an unknown endpoint (`endpoint-unknown`) and two named endpoints are this error (§12.6, N43), except a template upload where fan-out template upload is offered ([above](#a-fan-out-template-upload)), and a stored-query `PUT` or version `GET` where definition fan-out is offered beside the registry ([above](#a-distributed-stored-query)); so is a DEMOGRAPHIC request naming two (§7a.1, N32). |
| `query-parameter-refused` | 400 | A request routed to one node carries a query parameter the ITS-REST operation it addresses does not declare, or `subject_id` or `subject_namespace` anywhere but on `GET {base}/v1/ehr`, where the gateway consumes both as resolution input. The gateway cannot tell an identifying value from any other, so it sends nothing; the message names the parameter by position, never by name or value (§5.4.1, N33). |
| `node-timeout` | 504 | The node a request was routed to, or a member the ask-all probe asked, did not answer in time (§11.2, §11.5). A member that did not answer may hold the `ehr_id`, so the probe names no owner; the message names that member. |
| `node-unreachable` | 504 | The node a request was routed to, or a member the ask-all probe asked, could not be reached (§11.2). |
| `node-refused` | 424 | The node a request was routed to, or a member the ask-all probe asked, refused the gateway's onward credentials (§11.2). |
| `targeting-conflict` | 400 | The request names its node set twice, and the two sets differ: the AQL directive and a targeting header, or the endpoint header and the organisation header. The gateway never merges them and never picks one (§8.4.1, N35). The message names both sets by the registry endpoints each selects; every identifier in it is one the registry already holds. Two mechanisms that select the same set are accepted. A DEMOGRAPHIC request whose header names an endpoint other than the one the deployment declared for that area is this error too, naming both (§7a.1, N32). |
| `ehr-id-invalid` | 400 | The `ehr_id` in the request path, or the one `ehr_id` a query is scoped to (N29), is not an openEHR `HIER_OBJECT_ID`, so it names no EHR and the request is not routed (§12.5). The message never quotes the value. |
| `node-error` | 424 | A member the ask-all probe asked answered with neither a success nor `404`, so whether it holds the `ehr_id` is unknown and the read is not served (§11.2, §12.5.1). The message names the member and its status. |
| `probe-requires-uuid` | 400 | A read of an EHR resource, or a query scoped to one `ehr_id` (N29), that no targeting header, resolution binding or `ehr_id` index entry routes to one node has an `ehr_id` that is not a bare UUID: an ISO OID, an internet id, or a UUID with an extension. The gateway cannot tell such a value from a patient identifier, and the ask-all probe would send it to every member, so it asks no member anything (§5.4.1, N33, §12.5.1). Name the node in `openEHR-federation-endpoint`. The message never quotes the path. |
| `query-name-invalid` | 400 | A stored query's name is not `[{namespace}::]{query-name}` over `a-z`, `A-Z`, `0-9`, `_`, `.` and `-`, or its query name is `aql`, which ITS-REST reserves. The message never quotes the name. |
| `query-version-invalid` | 400 | A stored query's version is not `major.minor.patch` with no leading zeros, or, where a version is looked up, not a `{major}` or `{major}.{minor}` prefix either (ITS-REST, "Qualified query name"). |
| `query-version-required` | 400 | A definition is `PUT` at `{base}/v1/definition/query/{name}` with no version. The registry stores a definition only at a version, because a stored version is immutable (§12.7, N44). |
| `query-type-unsupported` | 400 | A definition's `query_type` is not `AQL`; the registry stores AQL only. |
| `subject-literal` | 400 | A definition names its patient by a literal identifier. The registry would hold that identifier at rest, so it refuses the definition; name the patient through a `$parameter` and bind it in `query_parameters` when you invoke the query (§5.4.1, N33). |
| `stored-query-held` | 409 | The registry already holds a definition at this name and version. A stored version is immutable, so the second `PUT` is refused and the held text stands; store the change as a new version (§12.7, N44). |
| `stored-query-unknown` | 404 | The registry holds no stored query at this name, or none at the version or version prefix the path names. |
| `preceding-version-invalid` | 400 | A versioned write names no single version it amends: `If-Match` is absent, repeated, a list, `*`, a weak tag or unquoted, or names no `OBJECT_VERSION_ID`; or the path of a composition `DELETE` is no `OBJECT_VERSION_ID`; or a `CONTRIBUTION` body is not one, in the representation its `Content-Type` selects (canonical JSON, or a canonical envelope whose `data` is FLAT or STRUCTURED), whose every `preceding_version_uid` is an `OBJECT_VERSION_ID`; or it is a `CONTRIBUTION` in canonical XML, which the gateway does not read yet (#308). Without that version the gateway cannot find the write's controlling CDR, so it sends nothing (ITS-REST 1.1.0 `If-Match` and `contribution_create`; §12.4, N23). |
| `parameter-value-invalid` | 400 | A request routed to one node carries a value that is not what the ITS-REST operation declares for it: a path `version_uid` that is no `OBJECT_VERSION_ID`, a `versioned_object_uid` that is no UUID, a `version_at_time` that is no extended ISO 8601 date-time, or a `detail_level` outside its listed values. The gateway sends nothing; the message names the header, or the parameter by its position and declared name, and never quotes the value (§5.4.1, N33). |
| `media-type-not-acceptable` | 406 | The `Accept` header of a request routed to one node admits none of the media types the ITS-REST operation answers in, as a node would answer it (RFC 9110 §12.5.1). The message lists the media types the operation offers. |
| `media-type-unsupported` | 415 | The `Content-Type` header of a request routed to one node is not one of the media types the ITS-REST operation takes, or carries a parameter other than `charset=utf-8` (RFC 9110 §8.3), or a body arrives without one for an operation whose body ITS-REST declares in several media types. The message lists the media types the operation takes. |
| `subject-several` | 409 | The subject of `GET {base}/v1/ehr` resolves at more than one member, and no `openEHR-federation-endpoint` header names one of them. The gateway never chooses by where the patient resolved, so it sends the read to none; the message lists the endpoints and never the subject. Name the endpoint in the header to read that member's EHR (§8.4, §12.5.2). |
| `resolution-unavailable` | 424 | The cross-reference service could not answer for a member while resolving the subject of `GET {base}/v1/ehr`, or no cross-reference service is configured. That member may hold the EHR, so the gateway answers neither its `404` nor another member's EHR; the message names the members and never the subject (§5.2, §11.2). |
| `ehr-id-held` | 409 | `PUT {base}/v1/ehr/{ehr_id}` names one member in `openEHR-federation-endpoint` while your session's resolution bindings or the gateway's `ehr_id` index already place that `ehr_id` at another member. Creating it would put one `ehr_id` at two members, the collision of §12.5.2, so the gateway sends the create to no node. The message names the holding endpoints and the one you named, and never quotes the `ehr_id`. When the member you named is the one that holds it, the create is sent there and that node answers its own `409` (ITS-REST 1.1.0 `ehr_create_with_id`; §12.4, §12.5.2, N23, N42). |
| `definition-endpoint-targeted` | 400 | A stored-query `PUT` names members to distribute the definition to, and its AQL carries a `FROM ENDPOINT` or `ORGANISATION` directive. The directive names members of the federation, which a node cannot run, so the gateway distributes nothing and stores nothing; store it without naming members and it runs federated (§12.7, §8.1, N44). |

The gateway answers `409` with five codes, and none of them sends anything
to a node:

- `ehr-id-collision`: two members claim one `ehr_id`, on a read or a write.
  The gateway also raises an integrity incident for the federation operator,
  because two nodes holding one `ehr_id` is a defect in the federation
  (§12.5.2, N42).
- `ehr-id-held`: a new EHR would take an `ehr_id` another member already
  holds. The refusal keeps the collision from arising, so it raises no
  integrity incident; the gateway logs a warning naming the endpoints
  (§12.4, §12.5.2).
- `controlling-system-unreachable`: a versioned write would be committed at
  a node other than its controlling CDR, which would fork the object (§10.3,
  N23).
- `subject-several`: the subject of `GET {base}/v1/ehr` resolves at more
  than one member. This is no integrity defect: a patient may have an EHR at
  several members, each under its own `ehr_id`, and the read by subject
  returns one EHR, so the gateway asks you to name the member (§12.5.2).
- `stored-query-held`: the registry already holds the stored query's name
  and version, and a stored version is immutable (§12.7, N44).

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
| `node-set-undefined` | 400 | The query names no patient, and neither the directive nor a targeting header names endpoints, so no node set is defined (N4, §8). |
| `endpoint-variable` | 400 | The variable of the `FROM ENDPOINT` directive is bound again in `FROM`, or used anywhere but as a selected column: in `WHERE`, in `ORDER BY` or inside a function. A path through it selects an ENDPOINT attribute (§8.1, §9.3). |
| `endpoint-attribute-unknown` | 400 | A path through the `FROM ENDPOINT` variable selects no ENDPOINT attribute: the attributes are `p/id` or `p/endpoint_id`, `p/organisation` or `p/organization_id`, `p/system_id` and `p/url`, each with no predicate and nothing after it (§9.3). |
| `endpoint-name-collision` | 400 | An ENDPOINT attribute column has the name of an EHR-derived column, so one name would denote two columns. Alias the attribute to a name no other column carries (N18, CP-35). |
| `incomparable-distinct-key` | 400 | Under `DISTINCT` with `ORDER BY` and `LIMIT`, a selected path names a value AQL defines no order for: a whole RM object (`SELECT DISTINCT c`), a data value that is not a `DV_ORDERED` (`c/name`, a `DV_TEXT`), the `DATA_VALUE` of an `ELEMENT`, a collection, or a path the RM does not resolve. AQL orders only primitives and `Ordered` types (AQL §ORDER BY), so a node cut at its `LIMIT` could keep different rows on each repeat (§11.6.1). Select a path to a primitive value, such as `c/name/value`, or drop the `LIMIT`. |
| `incomparable-order-key` | 400 | Outside `DISTINCT`, with `ORDER BY` and `LIMIT` (or `TOP`, the `fetch` member, or a bounded `OFFSET` page), an `ORDER BY` path names a value AQL defines no order for: a whole RM object (`ORDER BY c`), a data value that is not a `DV_ORDERED` (`c/name`, a `DV_TEXT`), the `DATA_VALUE` of an `ELEMENT`, a collection, or a path the RM does not resolve. AQL orders only primitives and `Ordered` types (AQL §ORDER BY), so the first rows of a node need not hold the federated first rows, which §11.6.1 shows only under a total order. Order on a path to a primitive value, such as `c/name/value`, or drop the `LIMIT`: without one, the gateway orders every row itself. |

## Other answers

Two answers come from the HTTP layer around the gateway and carry no error
body or code: `408`, with an empty body, when a request runs past the
server's request timeout, and `413`, with a short plain-text body, when the
request body is over the configured size limit.
