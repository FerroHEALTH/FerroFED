<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuration

The `ferrofed` binary reads one TOML file and the environment. It serves the
process shape (health, readiness, the request log, graceful shutdown) and, once
a registry is configured, the federated query `POST /v1/query/aql`, the EHR
resources and the definition area routed to one node, and, when
`[stored_queries]` is set, the stored-query registry; every other path under
`/v1/` answers `501`.

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
cannot carry: a bearer token that is not a `b64token` (RFC 6750 §2.1: letters,
digits, `-`, `.`, `_`, `~`, `+` and `/`, then any `=` padding), or a basic
user or password holding a control character such as a newline (RFC 7617
§2), or a basic user holding a colon. That refusal names the key the value came from, and never
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

## The registry document in FHIR form

The specification recommends the FHIR `Endpoint` and `Organization` resources
for the registry (N19). Set `registry.format = "fhir"` and `registry.document`
names a FHIR R4 JSON `Bundle` of type `collection` or `searchset` instead,
holding only `Organization` and `Endpoint` resources: the shape an mCSD
directory delivers (§15.1). The default, `registry.format = "toml"`, is the
native form above.

```toml
[registry]
document = "/etc/ferrofed/registry.json"
format = "fhir"
```

The form loads into the same members as the native form, and the gateway
routes over it identically. FHIR has no place for a node or an openEHR
`system_id`, and a resource's logical id belongs to the server that holds it,
so FerroFED carries the registry's ids as identifiers in its own systems (no
specification governs these systems; they are FerroFED's design):

| FHIR element | Registry fact |
|---|---|
| `Organization.identifier` with system `https://ferrofed.eu/fhir/sid/organisation-id` | the organisation id, exactly one |
| `Organization.name` | the organisation's display name |
| `Organization.endpoint` | the endpoints whose node the organisation operates |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/endpoint-id` | the stable endpoint id used in directives (N19), exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/node-id` | the node the endpoint belongs to, exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/system-id` | that node's openEHR `system_id`, exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/creating-system-id` | each further `creating_system_id` the endpoint answers for (N21), zero or more |
| `Endpoint.connectionType` | `openehr-rest-query` in `https://ferrofed.eu/fhir/CodeSystem/connection-type` |
| `Endpoint.managingOrganization` | the one managing organisation (N20) |
| `Endpoint.status` | `active`, or `suspended` for an endpoint taken out of service |
| `Endpoint.address` | the ITS-REST base URL |

An endpoint for the openEHR Query API never carries `hl7-fhir-rest` (§15.2).
No openEHR or HL7 code for it is registered yet, so FerroFED binds the one
code N19 names, `openehr-rest-query`, in a code system of its own. The mCSD
4.0.0 `Endpoint` profile binds `connectionType` to the HL7 endpoint connection
types extensibly, so a code from another system is admitted where the value
set has none for the purpose.

References resolve inside the Bundle as FHIR R4 §2.36.4.1 resolves them: a
relative `Organization/org-a` against the root of a REST `fullUrl` such as
`https://registry.example.org/fhir/Endpoint/node-a-pub`, and an absolute
reference, a `urn:uuid:` included, against an entry's `fullUrl`. Give every
entry a `fullUrl`. Other elements (`payloadType`, `period`, `header` and the
rest) are not read. A node's `product`, `version` and node identifiers have
no place in this form; a registry that needs them uses the native form.

`config check` refuses the document with the configuration exit code, naming
the resource, when:

- an endpoint's `connectionType` is `hl7-fhir-rest`, carries no system (an
  informal string), or is any other system and code (N19, §15.2). CP-20 is
  an operator point, and this check is how the gateway helps the operator
  meet it;
- an endpoint has no `managingOrganization`, or one that names no
  `Organization` of the Bundle (N20);
- an endpoint is listed by no organisation, or by two;
- an organisation or an endpoint has no id in its system or more than one, or
  an id repeats;
- the endpoints of one node disagree on its `system_id` or its operator;
- an endpoint's status is neither `active` nor `suspended`, or an organisation
  is marked inactive;
- a resource carries a `modifierExtension`, which FerroFED does not read;
- anything the native form refuses: a duplicate `system_id`, an unusable base
  URL, or a `creating_system_id` that is a member's own.

Reading the members from an mCSD directory itself, and keeping them in step,
follows with its own issue (#86).

## Federation id

A gateway that federates names its federation, and refuses to boot without
the name:

```toml
[federation]
id = "rso-example"
```

The id is `federation.id` of the `OPTIONS {base}/` self-description (§7a.2,
N30). It has no default, because it is the deployment's to choose, and an
empty id is refused. It is named in the startup log line.

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

## The `ehr_id` index

The gateway also keeps an index of which member holds which `ehr_id`, shared
by every client. It learns an entry when a resolution finds the patient's
`ehr_id` at a member, and when a member answers a request under that `ehr_id`
with a success. A follow-up on a path `ehr_id` that names no node and has no
binding is routed by the index before the gateway falls back to asking every
member (§12.5.1). The index holds `ehr_id`s and member ids only, lives in
memory, and forgets the least recently used `ehr_id` once it is full:

```toml
[federation]
ehr_index_capacity = 100000   # ehr_ids held, the default; 0 is refused
```

A forgotten or never-learned entry costs a later request one fallback step,
never a wrong route: a read then asks every member, and a write is refused
until the client names its node. An `ehr_id` seen at two members is held at
both, the index raises the index-insert alarm of §12b.2 once (an
`IndexInsertCollision` incident, see [Integrity incidents](#integrity-incidents)),
and from then on it routes neither: a request for that `ehr_id` that names
no node is refused `409` (`ehr-id-collision`). A held collision has no expiry
of its own, because nothing the gateway observes shows that a node was
remedied. It lasts until the entry is forgotten as least recently used or the
gateway restarts; after that, a read probes every member again, and a
collision that still stands is found and reported again.

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
  definition request goes to the one node the targeting headers name, and
  `POST /v1/query/{name}` answers `501`. Whether the registry is offered is
  named in the startup log line.

The [client contract](../integrate/client-contract.md#stored-queries) says how
a client stores and invokes a query.

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
| `/v1/ehr/{ehr_id}` and below | routed to the one node `openEHR-federation-endpoint` names, and answered as that node answered; `501` when no registry is configured |
| `/v1/definition/` and below | routed to the one node `openEHR-federation-endpoint` names, never merged; without the header a `400`; stored-query definitions held at the gateway when `[stored_queries]` is set; `501` when no registry is configured |
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

Every other header the gateway sends to a node for a federated query is fixed
by the gateway: `Accept` and `Content-Type` (`application/json`),
`Authorization` (the endpoint's configured onward credential, when it has
one), and the `Host`, `Content-Length` and `Accept-Encoding` the HTTP client
writes. None is copied from the client request.

A request routed to one node (`{base}/v1/ehr/{ehr_id}` and below) carries the
same `Authorization`, `X-Request-Id`, `Host`, `Content-Length` and
`Accept-Encoding`, and each request header the matched ITS-REST operation
declares, of `Accept`, `Content-Type`, `If-Match`, `Prefer`,
`openehr-version`, `openehr-audit-details`, `openehr-template-id`,
`openehr-item-tag` and `openehr-version-item-tag`, only those that operation
lists. The list comes from the `openehr-its` parameter table, never from the
gateway's own copy. `Accept`, `Content-Type` and `Prefer` are composed by the
gateway as values the operation lists; every other declared header is the
client's value byte for byte (see [Declared values](#declared-values)). Every
other client header is stripped, the client's `Authorization` and
`x-request-id` and the federation's own headers included, and the outbound
gate reads every forwarded value.

## Declared values

A routed request resolves no patient, so the outbound gate has no identifier
to compare a forwarded value against. The gateway works from what the
operation declares instead, before anything is sent.

It composes three headers itself, so a node receives a value the operation
lists, in the operation's own spelling, and never the client's text:

- `Accept` is read as a list of media ranges with weights (RFC 9110
  §12.5.1). Each listed media type takes the weight of the most specific
  range that covers it, and the node receives the heaviest one, the first
  listed on a tie. `*/*`, or no `Accept` at all, sends the first media type
  the operation lists; ITS-REST declares no default media type, so the first
  listed is the gateway's own choice, `application/json` in every EHR
  operation. A range with a parameter other than `q` or `charset=utf-8`
  covers nothing. An `Accept` that admits no listed type is a `406`
  (`media-type-not-acceptable`), as a node would answer it.
- `Content-Type` is read as a media type (RFC 9110 §8.3). When its type and
  subtype are a listed value, the node receives that value. A
  `charset=utf-8` is accepted and dropped, since the listed value carries no
  parameter and JSON is UTF-8 (RFC 8259 §8.1). Any other parameter, or a
  type that is not listed, is a `415` (`media-type-unsupported`).
- `Prefer` is read as a list of preferences (RFC 7240 §2). The node receives
  only the preferences the operation lists, in their listed spelling: a
  preference name is compared without regard to case, its value exactly, and
  only its first instance counts. Any other preference is dropped and never
  refused, as RFC 7240 allows a server to ignore it.

It holds every other value to what the operation declares, and a value that
does not match is a `400` (`parameter-value-invalid`) with nothing sent:

- a path `version_uid` is an openEHR `OBJECT_VERSION_ID`, a text
  `uid_based_id` an `OBJECT_VERSION_ID` or a `HIER_OBJECT_ID`, and a path
  parameter the table states as a UUID, such as `versioned_object_uid` or the
  `uid_based_id` of a composition update, is a UUID in its canonical
  hyphenated form, each parsed by `openehr-base`. The path then travels as
  the client sent it, since an openEHR uid is never rewritten (N22). The
  `ehr_id` is parsed by the routing itself;
- a date-time, such as `version_at_time`, is an extended ISO 8601 date-time
  in the openEHR BASE sense, with an offset only when needed (ITS-REST
  Overview, "Datetime format"), read by the `openehr-base` parser;
- an enumerated query value, such as `detail_level`, is exactly one of the
  values the operation lists;
- a UUID, an integer, a number and a boolean are each parsed as one.

The refusal names the header, or the path or query parameter by its position
and declared name, and never the value (§5.4.3). A value of such a kind
carries only what the kind admits: a four-digit year is a valid partial
date-time, and a `HIER_OBJECT_ID` admits a bare number as a one-arc ISO OID.

ITS-REST states the identifier class of `version_uid` and of a text
`uid_based_id` only in their descriptions, which the `openehr-its` table does
not carry yet, so the gateway reads those two names itself until it does
([FerroHEALTH/FerroEHR#3539](https://github.com/FerroHEALTH/FerroEHR/issues/3539)).

The parameter table states no kind for the rest, so the gateway cannot
classify them and forwards them as the client sent them. In the EHR area they
are the headers `If-Match`, `openehr-audit-details`, `openehr-item-tag`,
`openehr-template-id`, `openehr-version` and `openehr-version-item-tag`, the
path parameter `key` of an item tag, and the query parameters `path`,
`tag_key`, `tag_value` and `tag_target_path`.
N33 forbids an identifier in the parts of a request the gateway composes
(§5.4.1). Whether a client value the gateway forwards unchanged is one of
those parts is a question the specification leaves open, recorded on
[#212](https://github.com/FerroHEALTH/FerroFED/issues/212).

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
held. Every panic in `ferrofed serve`, inside a request or not, writes one
more line, "a thread panicked", with the source location and, inside a
request, the gateway's `request_id`. The panic message is never written:
the gateway replaces Rust's default panic hook, which prints it to stderr,
so a panic writes nothing to stderr. A federated query the gateway fails with a `500` also logs "the
federated query failed" with its error code and the same `request_id` as its
request line.

## Integrity incidents

A federation integrity defect is reported to you, the federation operator, as
an incident: one `ERROR` line under the log target `ferrofed::integrity`,
written once when the gateway detects the defect (§12.5.2, §12b.2, N42). The
line carries a stable `kind`, the routing ids involved and a message, and
never a request body, a header value or a patient identifier. These kinds
reach the log today:

| `kind` | When | Fields |
|---|---|---|
| `EhrIdCollision` | A request addressed an `ehr_id` that two members or more claim, and was refused `409` (`ehr-id-collision`). One line per refused request. | `ehr_id`, `detection` (`binding`, `index` or `ask-all`: the routing step that found the claimants), `claimants` (their endpoint ids) |
| `IndexInsertCollision` | The `ehr_id` index learned an `ehr_id` it already held at another member: the index-insert alarm of §12b.2. One line when the second claimant is learned, and one more for each further claimant. | `ehr_id`, `claimants` (the member node ids) |
| `LearnedCreatingSystemConflict` | A `creating_system_id` the registry document does not map was seen at two nodes, so the route learned for it is withdrawn and neither node is routed on (§12.2, N21). One line when the route is withdrawn. | `creating_system_id`, `first_endpoint_id`, `second_endpoint_id` |

The `ehr_id` is node-local and names no patient (§5.2), so the line names it
when it is a bare UUID. Any other form could be a patient identifier a client
wrote in a path, so the line then leaves the `ehr_id` field out.

Two nodes holding one `ehr_id` breaks the identifier-integrity conditions of
§12b.2, which admission should have checked, so the remedy is at the node.
The `claimants` name the members that hold the `ehr_id`. Have the node that
issued or adopted it in error fix it, then restart the gateway, which forgets
the collision the index holds (the index also forgets it when the entry is
the least recently used one past the index capacity). Until then, requests
that name no node are refused, and a client can still reach one of the
members by naming its endpoint in the `openEHR-federation-endpoint` header.

The gateway has no metrics endpoint, so the log is the record: count the
incidents by filtering the target `ferrofed::integrity` and grouping on
`kind` in your log pipeline. The request line of a refused request carries
its `409` and its `request_id`; the incident line does not name the request.
