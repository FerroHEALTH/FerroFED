<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Identity resolution

A patient query names its patient by an identifier and that identifier's
issuing namespace. The gateway resolves the pair, outside the query, to the
local `ehr_id` each member holds for the patient, and sends each member a
query scoped to that `ehr_id` alone (§5.2, N3, N7). This page covers the
cross-reference the gateway asks and how you configure it. Which members are
asked at all is [Localization](localization.md), and which may not be is
[Consent](consent.md).

See how it works: [where the patient identifier stops](../how-it-works/identifier-hygiene.md).

## Where the identifier comes from

The gateway reads the patient on either carrier §5.4.3 names (N33, CP-38):

- the subject predicate of §7,
  `e/ehr_status/subject/external_ref/id/value = '…'`, with the namespace in
  `e/ehr_status/subject/external_ref/namespace`;
- an `ENTRY`-level subject, `…/subject/identifiers/id = '…'`, with the
  namespace in `…/subject/identifiers/issuer` or `…/subject/identifiers/type`.

`GET {base}/v1/ehr?subject_id=…&subject_namespace=…` names the patient in
its query string instead
([Reading an EHR by subject](../integrate/follow-ups.md#reading-an-ehr-by-subject)).

A query that names no namespace resolves in `federation.default_namespace`
when you set one, and is refused `400` (`no-namespace`) when you do not.
§5.2 requires a namespace and names no default, so the setting is
FerroFED's own:

```toml
[federation]
default_namespace = "urn:oid:2.999.1"
```

## A PIX Manager: `[pixm]`

The identity binding FerroFED ships is IHE PIXm, the Patient Identifier
Cross-reference Manager's ITI-83 query (Annex A.1). Each registry member is
resolved by one Manager, in its `ehr_id` domain there: the assigning
authority whose identifiers are that member's `ehr_id`s.

```toml
[[pixm.manager]]
url = "https://pix.example.org/fhir/"

[pixm.manager.members]
"hospital-a" = "urn:oid:2.999.10"   # a registry node id = its ehr_id domain
"clinic-b" = "urn:oid:2.999.20"

[pixm.manager.credentials]
bearer_token_file = "/run/secrets/pix-token"

[pixm.namespaces]
"2.999.1" = "urn:oid:2.999.1"       # a client namespace = the PIX assigning authority
```

- Name one `[[pixm.manager]]` per Manager. Every member of the registry is
  resolved by exactly one: a member no Manager names, a member two Managers
  name, and a name the registry does not hold each refuse the configuration.
- Each domain, and each value of `[pixm.namespaces]`, is an absolute URI.
- `url` is the Manager's FHIR base, `https`, with no query or fragment. The
  Manager is asked for patient identifiers, so `http` is refused naming
  `pixm.manager[N].url`, with or without credentials, unless the profile is
  `development`
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)). A user name or password in it refuses the configuration, naming
  `pixm.manager[N].url` and never quoting it; the credentials go in
  `[pixm.manager.credentials]`, with the keys of an endpoint's
  [credentials](configuration.md#the-file): `bearer_token`, or `user` and
  `password`, each with its `_file` sibling. Leave the section out when the
  transport, such as mutual TLS or a private network, authenticates the
  gateway.
- `[pixm.namespaces]` maps the namespace a client writes to the assigning
  authority the Manager knows it by. A namespace that is itself an absolute
  URI needs no entry.
- `method` is `"get"`, the default, or `"post"`, which keeps the patient
  identifier out of the request URL
  ([`GET` or `POST`](configuration.md#a-pix-manager-asked-by-get-or-post)).

For each query, the gateway asks each Manager once, with one `targetSystem`
per member that Manager resolves, and reads the identifier the answer holds
in a member's domain as that member's `ehr_id`. The patient identifier goes
to the Manager and nowhere else; no node, log line or error body carries it
(§5.4.1, N33).

| The Manager answers, for one member | The member is | The query |
|---|---|---|
| exactly one identifier in the domain, which reads as an `ehr_id` | sent its node query | goes on |
| no identifier in the domain, or the patient is unknown | `not-resolved` | goes on; `complete` is false (N6) |
| two identifiers, one that is no `ehr_id`, an error, no answer within the budget, or a namespace with no mapping | `not-resolved`, with the reason in `error` | fails `424` under all-or-nothing |

A member the Manager could not answer for may hold the patient, so the
gateway fails the query by default rather than answer without it (§11.1;
§11.3 covers only an answered lookup, so this rule is FerroFED's own). With
`openEHR-federation-completeness: partial`, the members that did resolve
answer. Resolution runs inside the request's overall budget
([Timeouts](queries-and-areas.md#timeouts)).

### `[pixm]` as the localizer

Under `federation.node_selection = "localized"` with no `[xcpd]`, the
`[pixm]` resolver is also the localizer, the "demographic-registration" kind
of §14.2. The candidates are the members whose domain holds an identifier
for the patient at its Manager. Every other member is `not-localized` and is
never asked. The resolution of the same query reuses that ITI-83 answer, so
each query still asks each Manager once.

A member whose domain holds two identifiers, or one that is no `ehr_id`, is a
candidate, because the Manager holds the patient there; its resolution then
reports why it could not be asked. A Manager that fails, answers in a form
the gateway cannot read, or does not answer within
`[federation.localization] timeout_ms` fails the localization closed
(§14.1): every member is `not-localized` with the error, and no member is
asked unless `on_failure = "ask-all"`. `OPTIONS {base}/` declares
`localization.mode` as `"pixm"`. With `[xcpd]` set, XCPD localizes and
`[pixm]` only resolves.

## Demographics first: `[pdqm]`

A client may name the patient by an identifier the cross-reference does not
know, such as a hospital-local number. Annex A places a PDQm query ahead of
localization and resolution for that case (Annex A §A.2 and §A.7): the
gateway asks a Patient Demographics Supplier which person the identifier
names, takes the identifier that person carries in the master domain, and
localizes and resolves that identifier as it would the client's own.

```toml
[pdqm]
url = "https://pdq.example.org/fhir/"
transaction = "iti-78"                 # or "iti-119"
master = "urn:oid:2.999.1"             # the master domain's identifier system
timeout_ms = 1000                      # each exchange with the Supplier

[pdqm.namespaces]
"urn:oid:2.999.7" = "urn:oid:2.999.7"  # a client namespace = the system it is sent in

[pdqm.credentials]
bearer_token_file = "/run/secrets/pdq-token"
```

- Only an identifier in a namespace `[pdqm.namespaces]` names goes to the
  Supplier. Every other one is resolved as it is, and a namespace
  `[pixm.namespaces]` maps is refused here, naming the key.
- `transaction = "iti-78"`, the default, sends the Mobile Patient
  Demographics Query: `identifier=<system>|<value>` and
  `identifier=<master>|`, which asks the Supplier for identifiers in the
  master domain alone (PDQm §2:3.78.4.1.2.3). `"iti-119"` sends the Patient
  Demographics Match instead, with the identifier on the input Patient and
  `onlyCertainMatches` set (§2:3.119.4.1.2); declare it only where the
  Supplier offers `$match`.
- `master` is the identifier system of the master domain. The master
  identifier is resolved in the namespace of the same name, so `[pixm]` or
  `[dev]` must resolve it; `[pdqm]` without either refuses the
  configuration.
- `url`, the credentials and the `http` rule are those of a PIX Manager: the
  Supplier is sent the patient identifier, so plain `http` is refused,
  naming `pdqm.url`, unless the profile is `development`. No redirect is
  followed, and an answer is read up to 8 MiB.
- A `SIGHUP` reload applies a change to `[pdqm]`, as it does to `[pixm]`:
  the reloaded federation asks the Supplier the new table names.

| The Supplier answers | The members are | The query |
|---|---|---|
| one active Patient carrying exactly one identifier in the master domain (for ITI-119, graded `certain`) | localized and resolved by the master identifier | goes on |
| no Patient, or one with no master identifier (PDQm §2:3.78.4.1.3 Case 3, §2:3.119.4.1.3 Cases 4, 5 and 7) | `not-resolved` | goes on; `complete` is false (N6) |
| more than one Patient, a Patient with two master identifiers, or an ITI-119 match not graded `certain` | `not-resolved`, with the reason in `error` | fails `424` under all-or-nothing |
| a failure, or no answer within `timeout_ms` | `not-localized` with the error under `on_failure = "closed"`; `not-resolved` under `"ask-all"` and on a directed query | `200` under `"closed"` (§14.1); `424` otherwise |

A Patient with `active` set to `false` is a deprecated record and never a
match (§2:3.78.4.1.3 Case 6). The gateway never picks one of several
matches: ITI-78 counts them in `Bundle.total` (§2:3.78.4.1.3 Case 1), and
ITI-119 returns one entry for each (§2:3.119.4.1.3 Case 2). The step feeds
localization, so an outage on an undirected query follows the localization
failure policy, `closed` by default, as a localizer outage does; an exchange
whose audit record cannot be stored fails closed under every policy. The
read of an EHR by subject answers `404` for no match and `424` for the
rest.

The client's identifier reaches the Supplier and nowhere else; the master
identifier reaches the cross-reference and nowhere else. The outbound gate
withholds both from every request to a node, and no log line, metric label
or error carries either (§5.4.1, N33). Each exchange is audited through
[`[audit]`](audit.md), which is required outside development. The step's
state shows as `demographics` on
[`GET {base}/health/dependencies`](health.md), and each call is counted in
`ferrofed_demographics_requests_total` ([Metrics](metrics.md)).

## The development cross-reference: `[dev]`

For a laptop or a test, the gateway can resolve from a static table in the
configuration instead. It binds nothing, it is no identity binding of N3, and
it is accepted only in a configuration that declares itself for development
(no specification governs this: our own design):

```toml
profile = "development"

[[dev.crossref]]
namespace = "urn:oid:2.999.1.1"
value = "ffd-test-0001"
member = "node-a"
ehr_id = "aaaaaaaa-aaaa-4aaa-8aaa-000000000001"
```

Each row maps one synthetic patient to its `ehr_id` at one member; a patient
with no row at a member is `not-resolved` there. Under any other `profile`,
`[dev]` refuses the configuration. A development deployment prints a red
notice in its [startup banner](configuration.md#the-startup-banner) that it
must not hold or reach real patient data. The
[quickstart](container.md#the-quickstart) runs on this table.

## Choosing one

Set `[pixm]` or `[dev]`, never both: both refuse the configuration. With
neither, the gateway still starts, and every patient query fails closed: each
member is `not-resolved` with "no cross-reference service is configured",
and the query is a `424`. A query that names no patient needs no resolution
and runs as written.

Both sections take effect on a
[reload](registry.md#reloading-the-registry). `GET {base}/health/dependencies`
reports the resolver's last observed state
([Health probes](health.md)).

Under `federation.node_selection = "localized"` the `[dev]` table is also the
localizer: it names the members that hold a row for the patient, and every
other member is `not-localized`
([Node selection](registry.md#node-selection)).

## The identity feed: `[pmir]`

A merge at the identity source can leave a caller's
[resolution bindings](registry.md) naming the `ehr_id` of an identity that no
longer exists. The bindings expire with their lifetime in any case. With
`[pmir]` set, the gateway also hears of each change as it happens: it
subscribes at your IHE PMIR Patient Identity Registry with ITI-94, and the
Registry sends every Patient Master Identity change to the gateway as an
ITI-93 message (PMIR 1.6.0, Annex A.4).

```toml
[pmir]
url = "https://pmir.example.org/fhir"                    # the Registry's FHIR base
callback_url = "https://gateway.example.org/pmir/feed"   # where the Registry sends the feed
path = "/pmir/feed"                                       # the route under {base}, the default
feed_token_file = "/run/secrets/pmir-feed-token"          # the token the Registry sends
# identifier_system = "urn:oid:2.999.1"                  # only Patients with an identifier here
# timeout_ms = 5000
# check_interval_s = 60

[pmir.credentials]                    # how the gateway authenticates to the Registry
bearer_token_file = "/run/secrets/pmir-registry-token"
```

- **The subscription.** At start the gateway creates a `Subscription` at
  `url` with a `message` channel to `callback_url` and a FHIR JSON payload.
  Its criteria is `Patient`, or `Patient?identifier=<system>|` with
  `identifier_system` set, so the Registry sends only the Patients that
  carry an identifier from that authority. Before it creates one, the
  gateway searches the Registry for its own (`GET
  [base]/Subscription?url=<callback_url>`) and adopts the one it finds, so a
  create the Registry answered late is never made twice. A Registry that
  answers that search `400` or `404` does not support it, and the gateway
  creates. Every `check_interval_s` the gateway reads the subscription back.
  It deletes and recreates one the Registry reports `error` or `off`, and
  recreates one the Registry no longer holds. Each failed check doubles the
  wait before the next, up to 32 times `check_interval_s`, and a check that
  succeeds resets it. If the Registry answers a create `201` with no
  `Location`, or with one outside `url`, the gateway cannot manage that
  subscription. It creates no other until a restart and reports the fault as
  `unmanageable`; delete that subscription at the Registry. On a drain the
  gateway stops checking, lets the check in flight end, and then deletes its
  subscription, found by search when it never learned where it was.
- **Authenticating the feed.** The subscription carries no credential for the
  feed (PMIR §2:3.94.5), so you agree the feed token with the Registry's
  operator and configure it on both sides. The Registry sends it as
  `Authorization: Bearer <token>`. A message without that token is answered
  `401` and changes nothing.
- **The route.** `POST {base}{path}` takes one ITI-93 message. The route
  sits outside the ITS-REST surface and its client authentication, so the
  path may not be under `/v1`, `/health` or `/.well-known`. A message that
  does not hold to the PMIR profiles is answered `400` (`415` for a media
  type other than FHIR JSON) with an `OperationOutcome`, and changes
  nothing. An applied message is answered with the ITI-93 response.
- **What a message changes.** Only routing state, never a record. A merge
  or a delete drops every binding of the `ehr_id`s its Patient carries in a
  member's `ehr_id` domain, the domains `[pixm]` maps. An update states the
  identity as it now is and never what it lost, so it drops every binding,
  as does a merge or delete that carries no such `ehr_id`. A create drops
  nothing. A dropped binding costs one resolution, never a wrong node. The
  `ehr_id` index stays: a merge moves no EHR between nodes.
- **What is logged.** The kind of each change and their counts, and how many
  bindings went. The messages carry patient identifiers and demographics, so
  no identifier reaches a log line, a metric or an answer.

`url` and `callback_url` carry patient identities, so both are `https`
outside `profile = "development"`, and neither may carry a user name or a
password. `GET /health/dependencies` reports the Registry as
`identity_registry`: `up` while the gateway holds a subscription in
`requested` or `active`; `failing` after a refusal, an answer that breaks
ITI-94, or a create it cannot manage; and `down` when the Registry did not
answer. `identity_registry_fault` names the reason ([Health](health.md)). Each message is counted by result in
`ferrofed_identity_feed_messages_total` ([Metrics](metrics.md)). A change to
`[pmir]` takes a restart.

PMIR 1.6.0 defines no unmerge (§2:3.93.4.1.3), so the gateway hears of a
merge and never of a split. The specification marks this lifecycle track
provisional, and FerroFED claims no propagation of an identity change to a
later resolution: the [conformance matrix](../evaluate/conformance.md) keeps
track 8 deferred.
