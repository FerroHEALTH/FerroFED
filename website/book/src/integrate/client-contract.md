<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The client contract

A client of a federation gateway is an ordinary openEHR client. This page sets
out what the specification promises that client. Once a registry is
configured, FerroFED serves the federated query at
`POST {base}/v1/query/aql` and routes the EHR resources under a path
`ehr_id`, `{base}/v1/ehr/{ehr_id}` and below it, to one node (§7a.1). A
deployment that offers the stored-query registry also stores queries under
`{base}/v1/definition/query/` and runs them by name at
`POST {base}/v1/query/{name}` ([stored queries](#stored-queries), §12.7).
Every other ITS-REST path under `/v1/` answers `501` (N32).

## What a client sends

A conformant openEHR AQL request to `POST {base}/v1/query/aql`, where `{base}`
is the deployment's ITS-REST base URL; no prefix is mandated (§4.1, N28). The
patient is identified the openEHR way:

```sql
SELECT c/uid/value AS composition_id, c/context/start_time/value AS start_time
FROM EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = '12345'
  AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'
```

No federation-specific syntax is needed for a basic patient query (§3.2, N1).
A client that wants to pin a query to named systems can do so in the AQL,
with `FROM ENDPOINT …` or `ORGANISATION …`, or beside it, with a request
header (§8).

## Pinning a query to named systems

The gateway accepts both mechanisms of §8.4, and they select nodes the same
way (N35). Use either one; there is nothing to discover first, because every
conformant gateway accepts both (§7a.2).

Put the directive at the start of `FROM`, and the gateway asks exactly the
endpoints it lists (§8.1, N11):

```sql
SELECT c/uid/value AS composition_id
FROM ENDPOINT p [ "node_1", "node_2" ]
  CONTAINS EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = '12345'
```

`FROM ORGANISATION [ "org-a" ] CONTAINS …` asks every endpoint the registry
lists as managed by each organisation (N20). The identifiers are the stable
registry identifiers of `meta.federation.endpoints[].id`, never URLs (§8.1,
N19), and the keywords are case-insensitive like every AQL keyword.

- The directive does not replace finding the patient. Each listed endpoint
  is still asked about its own `ehr_id` for the patient (N7, N11). A listed
  endpoint where the patient is not known is reported `not-resolved`, and the
  query still answers `200` (§8.1, §11.3).
- Every registry endpoint the directive does not list is reported `excluded`.
  It is not asked and does not clear `meta.federation.complete` (§11.1).
- No node receives the directive. The gateway sends each node standard AQL,
  so the node query is the same as for the undirected request (N7).
- An identifier the registry does not know is refused `400` with
  `endpoint-unknown` or `organisation-unknown`, before any node is asked
  (§8.4.1). A known organisation that manages no endpoint leaves the request
  without a destination, `404` with `no-destination`.
- A query directed at one endpoint may use what a fan-out refuses: an
  aggregate, which goes to the node unchanged (N14, §11.6.3), and a function
  AQL does not define. Pinned to more than one endpoint, the same query
  follows the rules for an undirected one.
- The variable (`p` above) may only be selected, as an ENDPOINT attribute
  (§9.3). Using it in `WHERE`, in `ORDER BY` or inside a function, or binding
  its name again in `FROM`, is refused `400` with `endpoint-variable`.

Or leave the AQL as it is and send the node set in a header (§8.4). This is
the form for a stored query or a query a user wrote, since the query text
stays the same however it is targeted:

```http
POST {base}/v1/query/aql
openEHR-federation-endpoint: node_1, node_2
Content-Type: application/json
```

`openEHR-federation-organisation: org-a` is the organisation form. Each
header carries a comma-separated list of the same registry identifiers, and
may be sent as several field lines; empty list elements are ignored (RFC 9110
§5.6.1).

- The header selects nodes exactly as the directive does: `not-resolved` for
  a listed endpoint that does not know the patient, `excluded` for every
  other endpoint, `400` with `endpoint-unknown` or `organisation-unknown` for
  an identifier the registry does not know or a header with no identifier in
  it, and `404` with `no-destination` when the selection holds no endpoint.
  A query directed at one endpoint by the header may use an aggregate, as one
  directed by the AQL may.
- Send the directive and a header, or both headers, only when they select the
  same endpoints. Then the request proceeds. When they differ, it is refused
  `400` with `targeting-conflict`, naming both sets; the gateway never merges
  them and never picks one (§8.4.1, N35).
- No node receives either header, and no query parameter targets anything:
  `?endpoint=` and `?organisation=` on the federated query have no effect
  (§8.4).
- The headers apply to every federated request, the routed requests of
  [follow-ups](#follow-ups) included; the directive applies to AQL only
  (§8.4).

## Provenance columns: ENDPOINT attributes

A directed query can ask for the endpoint each row came from. Select an
attribute through the directive's variable, and the gateway fills it in for
every row from the registry entry of the endpoint that answered (§9.3, N12):

```sql
SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition_id
FROM ENDPOINT p [ "node_1", "node_2" ]
  CONTAINS EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = '12345'
```

| Path | Value in the row |
|---|---|
| `p/id`, `p/endpoint_id` | the endpoint id, as `meta.federation.endpoints[].id` reports it |
| `p/organisation`, `p/organization_id` | the endpoint's managing organisation (N20) |
| `p/system_id` | the openEHR `system_id` of the endpoint's node, the follow-up routing key (§12) |
| `p/url` | the CDR base URL the registry holds |

- Each value is a JSON string. No node is asked for it, and no node receives
  the variable or the directive (§8.1).
- A query that selects no attribute gets exactly the columns and rows a single
  CDR would return (N17).
- `columns[]` is the gateway's rendering of your query: an attribute column is
  named by its alias, and its path is the ITS-REST form with the variable
  stripped, `/id` for `p/id` (§9.2).
- An alias keeps an attribute apart from an EHR-derived column of the same
  name: `p/system_id AS node_system, e/system_id/value AS system_id` returns
  both (N18). Giving the attribute and an EHR-derived column the same alias is
  refused `400` with `endpoint-name-collision`, so no column is shadowed
  (CP-35). Any other path through the variable, such as `p/name` or
  `p/id/value`, is refused `400` with `endpoint-attribute-unknown`.
- Under `SELECT DISTINCT` the attributes count as selected columns: two rows
  of two endpoints are two rows when their attributes differ (N13).
- An attribute beside an aggregate over more than one endpoint would count per
  endpoint, so it is refused `400` with `indecomposable-aggregate`; pinned to
  one endpoint, the aggregate row carries the attribute (§11.6.3).
- Ordering on an attribute is refused `400` with `endpoint-variable`: §9.3
  defines the attributes as selectable only. The gateway already orders tied
  rows by endpoint id (§11.6.1).

## What a client gets back

An openEHR `RESULT_SET` exactly as ITS-REST 1.1.0 defines it, rows as ordered
arrays matched to `columns` (§9.1). A client that parses the AQL response of a
single CDR parses a federated one. Everything the federation adds lives in one
member of the open `meta` object, `meta.federation`:

- `complete`, whether the answer covers every node in scope (§11.4);
- `endpoints[]`, one entry per node with its status and provenance (§9.5,
  §11.1);
- `timeout`, the budget that applied (§11.5);
- `dedup`, the de-duplication policy that applied (§10).

By default you get every row every node returned, duplicates included
(§10.1, N15). Send `openEHR-federation-dedup: version-identity` to get one
copy of a composition version held at several nodes: the copy from the CDR
that created it when that CDR answered, else the one from the lowest
endpoint id. Version ids and system ids that differ only in case name the
same thing (openEHR BASE), and the kept row comes back as its node sent it.
The gateway then orders the version uid without regard to case too, in an
`ORDER BY` on it and as the tie-break, so the page a `LIMIT` cuts does not
depend on the case a node wrote it in (§11.6.1).
`meta.federation.dedup` then names the endpoints whose
copies were dropped and counts the rows (§10.2, §10.3). Two versions of one
composition are two rows either way, and `none` states the default
explicitly. Any other value is refused `400` with the code `dedup-invalid`.

Read whether an answer is complete from `meta.federation.complete`, never
from the status code; the gateway emits no FHIR `OperationOutcome`, because
its answer is an ITS-REST `RESULT_SET` (§11.4, N17).

A request that fails answers the status §11.2 names and a stable code; the
[errors and status codes](errors.md) page lists every one.

## Follow-ups

A composition id in a result row is an `OBJECT_VERSION_ID`, which already
carries the `creating_system_id` of the CDR that created it (§12.2). A new
object is always created on one node the client names; creation across
nodes is refused (§2.3, N23).

### Reading one version

A read whose path names one version, such as
`GET {base}/v1/ehr/{ehr_id}/composition/{uid_based_id}`, or a `version_uid`
under `versioned_composition`, `ehr_status`, `versioned_ehr_status` or
`directory`, goes to the node you name in `openEHR-federation-endpoint`, or
else to the node the path `ehr_id` resolves to, in the order below (§12.5.1,
N41). The version's `creating_system_id` does not route it: a request under a
path `ehr_id` is routed on that `ehr_id` (§12a.1), and the gateway never
rewrites the path for another node (N22). A copy imported into another node
carries the same version, so reading it where the row came from returns the
same content. To read the copy at the node that created it, send that node's
`ehr_id` for the patient, which a row from that node carries.

The gateway still learns which node holds versions of each
`creating_system_id` the registry document does not map (§12.2, N21). It
reads every version uid in a federated query's rows (as `c/uid/value`, as a
`uid`, or as the `uid` of a selected `COMPOSITION` or `VERSION`) and every
version uid a routed read names or answers with in its `ETag`. A system seen
at two nodes raises an integrity incident for the operator. These learned
routes never route a read under a path `ehr_id`, and never make a node that
holds a copy the CDR that controls it.

### Routing a path `ehr_id`

An `ehr_id` carries no system component, so a request to
`{base}/v1/ehr/{ehr_id}/…` does not say on its face which node holds the EHR
(§12.5). The `ehr_id` in the path must be an openEHR `HIER_OBJECT_ID`; any
other value is a `400` (`ehr-id-invalid`) and nothing is routed. The gateway
then finds the node in this order, and takes no later step once one names
exactly one node (§12.5.1, N41):

1. The targeting headers. Name the node yourself in the
   `openEHR-federation-endpoint` header, with the `endpoint_id` the result
   row carried (§8.4), or in the `openEHR-federation-organisation` header,
   when the organisation manages that one endpoint. This is the recommended
   way. Together the headers select exactly one registry endpoint: an unknown
   identifier, several endpoints or two headers that disagree are a `400`,
   and an organisation that manages no endpoint is a `404`
   (`no-destination`). A query parameter such as `?endpoint=` names no node
   here either; it is one the operation does not declare, so it is a `400`
   (`query-parameter-refused`) and nothing is sent.
2. A resolution binding of your client session: the node your earlier query
   resolved that `ehr_id` at. Bindings belong to an authenticated session, so
   this step answers once client authentication lands; until then it never
   does.
3. The gateway's `ehr_id` index, which it learns from resolutions and from
   the nodes' successful answers.
4. For a read only, an ask-all probe: the gateway sends
   `GET {base}/v1/ehr/{ehr_id}` to every member at once, within its per-node
   timeout and overall budget (§11.5). The one member that answers with the
   EHR, while every other member answers `404`, gets your read; a read of
   the EHR itself is answered from that probe. When every member answers
   `404`, the read is a `404` (`no-destination`). When two members hold the
   `ehr_id`, the read is a `409` (`ehr-id-collision`) that lists them, and
   neither is read (§12.5.2, N42). When a member does not answer in time,
   cannot be reached, or answers an error, the owner is unknown and the read
   fails: `504` (`node-timeout`, `node-unreachable`) or `424`
   (`node-error`, `node-refused`), naming the member. The probe is sent only
   for an `ehr_id` that is a bare UUID. Any other `HIER_OBJECT_ID` form, such
   as an ISO OID, could be a patient identifier, and the probe would carry it
   to every member, so the read is a `400` (`probe-requires-uuid`) and no
   member is asked (§5.4.1, N33). Such an `ehr_id` is routed by the first
   three steps only: name its node in the endpoint header, and the gateway
   forwards it to that node alone.

A binding or an index entry that names two members is a collision, and the
gateway never picks one of them: the request, a read or a write, is a `409`
(`ehr-id-collision`) that lists them, nothing is probed, and neither member
is sent it (§12.5.2, N42). The explicit target of step 1 still routes such an
`ehr_id` to the node you name. A write that none of the first three steps routes is a `400`
with the code `target-required`, and nothing is probed, because the gateway
never finds a write's destination by trial (§12.5.1, N41).

A routed request reaches the node as you sent it:

- the body byte for byte, a `DV_IDENTIFIER` in a committed `COMPOSITION`
  included, because a commit body is clinical content the gateway has no
  right to alter (§5.4, N33);
- the method and the path, under the node's own base URL;
- the request headers the ITS-REST operation you address declares (of
  `Accept`, `Content-Type`, `If-Match`, `Prefer` and the `openehr-*` commit
  headers, the ones that operation lists), and no other header. `If-Match`
  reaches the node on a `PUT`, for example, and never on a `GET`. Your
  `Authorization` never reaches a node: the gateway authenticates to each node
  with that node's own credentials (§13). Neither does your `X-Request-Id`:
  the node receives the gateway's own id for the request. Nor do the
  targeting headers, which mean nothing at a node (§8.4);
- the query string, when the operation declares every parameter in it (such
  as `version_at_time` on a read, or `path` on a directory read). Any other
  parameter is a `400` (`query-parameter-refused`) and nothing is sent,
  because the gateway cannot tell an identifying value from any other
  (§5.4.1, N33);
- each declared header and query value only when it is of the kind the
  operation states for it: a `version_at_time` in the extended ISO 8601
  format, a `Prefer`, `Accept` or `Content-Type` that is exactly one of the
  values the operation lists. Any other value is a `400`
  (`parameter-value-invalid`) and nothing is sent. Send `Accept` as one media
  type the operation lists, or leave it out: `*/*`, a list or a media-type
  parameter is refused. A value of free text, such as `If-Match` or a
  directory `path`, travels as you sent it (see
  [Configuration](../operate/configuration.md#declared-values)).

The answer is the node's: its status, its body, and its `Location` and `ETag`
unmodified, since openEHR uids are never rewritten (N22, N31). Every routed
answer, `POST`, `PUT` and `DELETE` included, names the acting endpoint in
`openEHR-federation-endpoint` and its node's `system_id` in
`openEHR-federation-system-id` (§7a.3, §9.6). A node's `Location` is the
node's own URL or path, so following it bypasses the gateway; send the
follow-up to the gateway with the version uid instead.

### Writing a new version

A versioned write amends a version that exists: an update of a composition,
of the `EHR_STATUS` or of the directory, a delete of the directory, each
naming the version it amends in `If-Match`, and a delete of a composition,
naming it in the path. It goes to the CDR that controls that version, the one
whose `system_id` equals the version's `creating_system_id`, and to no other
node (§12.4, §12a.1, N23). Writing at a node that holds only an imported copy
would fork the object (§10.3).

The gateway routes the write by its path `ehr_id`, in the order above, and
never by ask-all (§12a.1, N41). It then checks that node against the version:
the registry must map the version's `creating_system_id` to it, as the
node's own `system_id` or as a `[[creating_system]]` mapping the operator
registered. A mapping the gateway learned from answers never counts, because
a node that holds versions of a system need not have created them. The
outcomes:

- The path node controls the version: the write goes there once, as you sent
  it, and the node's answer comes back with its `ETag` and `Location`.
- Another member controls it, or no member is known to: the write is a `409`
  (`controlling-system-unreachable`), and no node is sent it. The message
  names the controlling node and its endpoint where the registry knows them.
  The path `ehr_id` belongs to the node it routes to, so the gateway cannot
  send the write to the controlling node instead; send it there under that
  node's own `ehr_id` for the patient.
- The write names no single version: `If-Match` is absent, repeated, a list,
  `*`, a weak tag, unquoted, or no `OBJECT_VERSION_ID`, or a composition
  delete's path is no `OBJECT_VERSION_ID`. That is a `400`
  (`preceding-version-invalid`), and nothing is sent.

A `CONTRIBUTION` names the versions it amends only in its body, which the
gateway never reads, so it is routed by its path `ehr_id` alone.

### Creating an EHR

A new EHR has no owner yet, so neither a binding nor the index can name its
node, and nothing is probed for it. `POST {base}/v1/ehr` and
`PUT {base}/v1/ehr/{ehr_id}` go only to the one endpoint you name in
`openEHR-federation-endpoint` or `openEHR-federation-organisation` (§12.4,
N23). Without a header the request is a `400` (`target-required`); headers
that select several endpoints are a `400` (`endpoint-several`), because an
EHR is created at one node only (§2.3). The body, its `EHR_STATUS` subject
included, reaches that node byte for byte, and the node's `Location` and
`ETag` come back unmodified. A composition or a directory created inside an
existing EHR is routed by its path `ehr_id` like any other request under it.

## Stored queries

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
  is a `400` (`query-name-invalid`, `query-version-invalid`). A `PUT` with no
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
  [targeting headers](#pinning-a-query-to-named-systems). A definition that
  carries `FROM ENDPOINT` or `ORGANISATION` is targeted by it, and a header
  that selects other endpoints is a `400` (`targeting-conflict`).
- A parameter you do not bind, or bind and the query does not use, is a `400`
  (`parameters`) naming it, never its value.
- The answer is the ordinary federated `RESULT_SET`, with `name` naming the
  gateway's stored query and `q` its stored text (§9.1, §12.7). No node
  receives your patient identifier, and no node receives the stored query by
  name: each gets the standard AQL of an inline query.

The gateway does not distribute definitions to the nodes, and
`definition.stored_query_fan_out` is `false` (§12.7). Templates are not
served: `{base}/v1/definition/template/…` answers `501`.

## Self-description

`OPTIONS {base}/` returns what the gateway does and which members stand
behind it, as JSON that validates against the specification's
`options-root.schema.json` (§7a.2, N30). It needs no patient identifier and
carries none. Client authentication is not built yet, so it answers any
caller; once it is, the body answers only an authenticated one (§7a.2, §13). Every value comes from the running configuration, so the body
says what the gateway does, not what it was once meant to do:

| Member | What FerroFED declares |
|---|---|
| `federation.id` | the deployment's `federation.id` |
| `federation.spec_version` | `0.9`, the `major.minor` of the pinned specification release |
| `aql.fan_out` | `true`: an undirected query asks every member (§4.3, N4) |
| `dedup` | `default: "none"`, `modes: ["none", "version-identity"]`, and the request header `openEHR-federation-dedup` (§10, N15) |
| `timeout` | the configured `per_node_ms` and `overall_ms`, with `policy: "abandon-and-mark"`: a node past its budget is abandoned and reported `time-out` (§11.5, N38) |
| `completeness` | `default: "all-or-nothing"`; `best_effort` and, when it is offered, `opt_in` naming `openEHR-federation-completeness: partial` (§11.4, N37) |
| `paging` | `offset_strategy: "bounded"` with the configured `max_window`, or `"reject"`; never `"cursor"`, because no cursor is offered (§11.6.2, N39) |
| `aggregates.decomposable` | the configured functions, of `COUNT`, `SUM`, `MIN`, `MAX` and `AVG`; an empty list means none (§11.6.3) |
| `definition` | `fan_out_template_upload: false` and `stored_query_fan_out: false`; `stored_query_registry` is `true` while `[stored_queries]` is set and `false` otherwise (N43, N44, §12.7) |
| `localization.on_failure` | `"closed"`: the gateway never widens to ask-all when a localizer fails (§14.1) |
| `its_rest` | `query` federated, `ehr` routed to the one node that owns the `ehr_id` (§12.5.1), `definition` held at the gateway registry for stored queries when it is offered and unsupported (`501`) otherwise, and `demographic` unsupported (`501`) |
| `endpoints[]` | every registry endpoint with its `id`, its managing `organisation`, its `status` (`active`, or `suspended` for one the operator took out of service), its `node_id` and `system_id`, and the node's `product` and `version` where the registry holds them |

What is absent is absent on purpose:

- No targeting mechanism and no patient-resolution carrier. Both forms of
  each are mandatory at every gateway, so there is nothing to choose
  (§7a.2, N33, N35).
- No asynchronous queries. The schema has no member for them, and the gateway
  does not offer them (§11.7).
- No `auth.jwks_uri`. The gateway publishes no JWKS yet, and the schema says
  a gateway with none configured omits the key (§13.1).
- No latency. The gateway keeps no latency statistic per member.
- `paging.max_window` is FerroFED's own member inside the open `paging`
  object: the specification names no member for the bound of the `bounded`
  strategy.

`OPTIONS` on a path under `{base}/v1/` answers `204` with the methods served
there in `Allow`: `POST, OPTIONS` for `/v1/query/aql`, and the ITS-REST
methods of the resource for an EHR resource under a path `ehr_id`, such as
`GET, PUT, OPTIONS` for `/v1/ehr/{ehr_id}`. Where the stored-query registry is
offered, a stored query answers `POST, OPTIONS` and a definition
`GET, PUT, OPTIONS`. The gateway answers it without
asking a node. A path the gateway does not serve answers `501`.
