<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The client contract

A client of a federation gateway is an ordinary openEHR client. This page sets
out what the specification promises that client. Once a registry is
configured, FerroFED serves the federated query at
`POST {base}/v1/query/aql` and routes the EHR resources under a path
`ehr_id`, `{base}/v1/ehr/{ehr_id}` and below it, to one node (§7a.1). Every
other ITS-REST path under `/v1/` answers `501` (N32).

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
- The variable (`p` above) may only be selected, as an ENDPOINT attribute such
  as `p/id` (§9.3). Selecting one answers `501` with `not-implemented`:
  adding those columns to the rows is planned build order. Using the variable in `WHERE`,
  in `ORDER BY` or inside a function, or binding its name again in `FROM`, is
  refused `400` with `endpoint-variable`.

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
carries the `creating_system_id` of the CDR that created it. A follow-up read
or write sent to the gateway is routed to that CDR (§7.2, §12). A new object
is always created on one node the client names; creation across nodes is
refused (§2.3, N23).

Today you name the node of a request to `{base}/v1/ehr/{ehr_id}/…` yourself,
in the `openEHR-federation-endpoint` header, with the `endpoint_id` the
result row carried (§12.5.1 step 1, §8.4), or in the
`openEHR-federation-organisation` header, when the organisation manages that
one endpoint. Together the headers select exactly one registry endpoint: an
unknown identifier, several endpoints or two headers that disagree are a
`400`, and an organisation that manages no endpoint is a `404`
(`no-destination`). A query parameter such as `?endpoint=` names no node
here either; it is one the operation does not declare, so it is a `400`
(`query-parameter-refused`) and nothing is sent. A write that names
none is a `400` with the code `target-required`, because the gateway never
finds a write's destination by trial (§12.5.1, N41). A read that names none
answers `501` until the gateway can find the node by itself.

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
  (§5.4.1, N33).

The answer is the node's: its status, its body, and its `Location` and `ETag`
unmodified, since openEHR uids are never rewritten (N22, N31). Every routed
answer, `POST`, `PUT` and `DELETE` included, names the acting endpoint in
`openEHR-federation-endpoint` and its node's `system_id` in
`openEHR-federation-system-id` (§7a.3, §9.6). A node's `Location` is the
node's own URL or path, so following it bypasses the gateway; send the
follow-up to the gateway with the version uid instead.

## Self-description

`OPTIONS {base}/` returns the gateway's self-description, including what it
refuses rather than approximates, validated against the specification's
`options-root.schema.json` (§7a.2).
