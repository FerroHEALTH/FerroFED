<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The audit trail

Every IHE transaction the gateway makes or receives is audited as its
profile requires, to an ATNA Audit Record Repository over ITI-20 Record
Audit Event. Two configurations decide where the records go:

- `[xcpd] audit` and `[xcpd.audit_repository]` for the XCPD localizer's
  ITI-55 exchanges, which XCPD audits as a DICOM audit message over syslog
  ([The audit repository](localization.md#the-audit-repository));
- `[audit]` for the PIXm, PDQm, mCSD and PMIR transactions, which each profile
  audits as a FHIR `AuditEvent` built on the IHE Basic Audit Log Patterns
  (BALP). BALP has the gateway send it over the ATX: FHIR Feed Option of
  ITI-20: a FHIR `create` at the repository's FHIR base (the RESTful ATNA
  supplement, ITI TF-2 §3.20.4.2).

## What each transaction records

| Transaction | When | Record | Patient named | Section |
|---|---|---|---|---|
| ITI-55 Cross Gateway Patient Discovery | each discovery the XCPD localizer makes | the Initiating Gateway's DICOM audit message, over syslog | inside the base64 query parameters | `[xcpd]` |
| ITI-83 Mobile Patient Identifier Cross-reference Query | each query the PIXm resolver or localizer asks a PIX Manager | PIXm Query Consumer audit (BALP Patient Query): the request as sent, base64; for `method = "post"`, its request line, media type and `Parameters` body | the source identifier, system and value | `[audit]` |
| ITI-78 Mobile Patient Demographics Query | each search the `[pdqm]` step sends the PDQm Supplier | PDQm Query Consumer audit (BALP Query): the request as sent, base64 | inside the base64 request; a demographics search identifies no patient on its own | `[audit]` |
| ITI-119 Patient Demographics Match | each match the `[pdqm]` step sends under `transaction = "iti-119"` | PDQm Match Consumer audit (BALP Query): the request as sent, base64 | the identifier on the input Patient, system and value | `[audit]` |
| ITI-90 Find Matching Care Services | each search of the mCSD directory the registry is read from, one per resource type | mCSD Care Services Query audit (BALP Query): the first request of the search, base64 | none | `[audit]` |
| ITI-91 Request Care Services Updates | each history of a registry refresh, one per resource type | mCSD Care Services Updates audit: the first request, base64 | none | `[audit]` |
| ITI-93 Mobile Patient Identity Feed | each message the Patient Identity Registry sends to the feed route | PMIR Feed audit, the Registry as the source and the gateway's callback as the destination: the `MessageHeader` | one entity per Patient Master Identity the message names, by its `Patient/<id>` reference or an identifier | `[audit]` |
| ITI-94 Subscribe to Patient Updates | each subscription create, read and delete the identity feed makes | PMIR Subscription Create, Read and Delete audits (BALP Create, Read, Delete): the `Subscription` and, for a create, its criteria | none: a subscription names an identifier system at most | `[audit]` |
| The search of the Registry's subscriptions by callback | before each create | BALP Query: the request as sent, base64; ITI-94 defines no search | none | `[audit]` |

Every record names the gateway as `source.observer` and as its own agent,
by `source_id` (the hostname by default), at the network address
`hostname` gives, and names the other party by its FHIR base URL without
its userinfo or query.

## The caller each record names

A transaction the gateway makes for a client's request is made on behalf
of the caller it verified ([Client authentication](authentication.md)).
PIXm asks that its audit record be augmented with the agent details of the
caller's OAuth token, following BALP (PIXm §2:3.83.5.2.1), so each such
record names the caller as BALP maps the token (BALP §3:5.7.5.4):

| Record | Where the caller is named |
|---|---|
| ITI-83, ITI-78 and ITI-119 (`AuditEvent`) | the `agent:user` of the BALP pattern: type `IRCP`, `who.identifier.system` the token's `iss`, `who.identifier.value` its `sub`, `requestor` true, `purposeOfUse` every purpose of use the token declares; and an Application agent (DICOM `110150`) with the token's `client_id` as `who.identifier.value` |
| ITI-55 (DICOM audit message) | the Human Requestor `ActiveParticipant` (ITI TF-2 §3.55.5.1.1): `UserID` the token's `sub`, `UserName` written `aud<sub@iss>` (IUA ITI TF-2 §3.72.5.1), `UserIsRequestor` true, and the gateway's own participant `UserIsRequestor` false. The DICOM schema has no element for the client or the purpose of use, so the message names neither |

A transaction the gateway makes on its own behalf names no caller: the
admission check's ITI-83 queries, every ITI-90 search and ITI-91 history
of a registry read or refresh, every ITI-94 subscription exchange and the
search before it, and every ITI-93 message the Registry sends. Their
`AuditEvent` has no `agent:user` and no Application agent, as BALP's
examples of an event no user caused have none, and an ITI-55 message of
its own names the gateway as the requestor.

The caller's identity goes to the Audit Record Repository alone. Both log
destinations, `[audit] destination = "log"` and `[xcpd] audit = "log"`,
record `on_behalf` as `caller` or `gateway`, and never the caller's `sub`,
`client_id` or issuer; no metric label carries them. Each
node is told of the caller by the signed conveyance alone
([Onward credentials](onward-credentials.md)), never by an audit record.

## The access log

Every access to patient data the gateway intermediates is recorded, with
the caller who made it. This is the GDPR record of who read and wrote
which patient's data, and the logging component Regulation (EU) 2025/327
asks of an EHR system: a record "on every access event or group of events"
(Annex II 3.2). The record goes where the `[audit]` records go, as a BALP
`AuditEvent`, through the same spool.

| Access | When it is recorded | BALP pattern | Action |
|---|---|---|---|
| A federated AQL query, `POST` or `GET {base}/v1/query/aql` | once at least one member was sent it; a query no member was sent reached no data | `IHE.BasicAudit.PatientQuery` when the query names a patient, `IHE.BasicAudit.Query` otherwise | `E` |
| A stored-query execution, `POST {base}/v1/query/{name}` | as for an ad hoc query, the stored query named in the record | as for an ad hoc query | `E` |
| A routed read: an EHR resource, the read of an EHR by subject, a DEMOGRAPHIC read | once a node acted for it (`openEHR-federation-endpoint` on the answer) | `IHE.BasicAudit.PatientRead` with the patient, `IHE.BasicAudit.Read` otherwise | `R` |
| A routed write: a create, an update or a delete under an EHR, the creation of an EHR, a DEMOGRAPHIC write | once a node acted for it | `Create`, `Update` or `Delete`, the `Patient` variant with the patient; a failed delete claims no pattern, since the Delete pattern fixes `outcome` to `0` | `C`, `U`, `D` |

A definition request (a template, a stored-query definition) reaches no
patient data, so it is not recorded. The operator console reaches the
gateway through the same ITS-REST surface, with the operator's own access
token, so a query run from the console is recorded with the operator as
the caller.

### What a record names

| Annex II 3.2 | Where the `AuditEvent` carries it |
|---|---|
| (a) the provider or other individuals who accessed the data | an agent whose `who` is an `Organization` by its identifier: the requester's organisation where the issuer states one ([Consent](consent.md)), the IHE IUA `subject_organization_id` otherwise; and the Application agent (DICOM `110150`) with the token's `client_id` |
| (b) the specific natural person who accessed the data | the `agent:user` (`IRCP`): the token's `iss` and `sub`, `altId` the professional identification the issuer states, and every purpose of use |
| (c) the categories of the data accessed | the entity named `ehds-categories`: see [Categories](#categories) |
| (d) the time and date | `recorded` |
| (e) the origin or origins of the data | one entity named `origin` per endpoint the access asked: the endpoint id, its node, its `system_id`, how it answered (`active`, `node-error`, `time-out`, and so on, or the node's HTTP status for a routed request) and the rows it sent |

Beside them, a record names the data subject: the patient by the
identifier and namespace the request named (`entity:patient`, as the ITI-83
record names it), and one entity named `ehr` per `ehr_id` the access
reached, with its endpoint. It carries the request as `entity:query`, the
gateway's request id as `entity:transaction` (the id the request log names
too), the address the request came from, and the outcome.

### Categories

A record names the categories of the data accessed: the six priority
categories of Art 14(1), by the codes `patient-summary`,
`electronic-prescription`, `electronic-dispensation`, `medical-imaging`,
`medical-test-result` and `discharge-report`, and the national categories
you declare. Neither AQL nor ITS-REST carries a category, so the gateway
reads the openEHR model ids of what an access reached and looks them up in
the category map you declare in `[access_log]`. FerroFED ships no map. No
specification governs the map: it is FerroFED's own design.

- The ids are read from the archetype roots the access delivered, read or
  wrote: the `archetype_details` of each `COMPOSITION`, or of each
  `ORIGINAL_VERSION`'s data, a delivered row holds, the `COMPOSITION` a
  routed read returns, and the `COMPOSITION` a write sends in canonical
  JSON. The body is read, never changed (N22).
- A template key wins over an archetype key. An archetype's `none` never
  stands for a template the map does not hold.
- Where the access delivered a leaf value, an aggregate or no row at all,
  the ids the bound query constrains its data to classify it too: its
  archetype predicates, and `=` on `archetype_node_id` and
  `archetype_details/template_id/value` in the top-level `AND` chain of
  `WHERE`, read from the syntax tree, never from a comment, a string
  compared with another path, `NOT`, `!=` or `NOT CONTAINS`.
- An access spanning categories records all of them, and `none` never
  removes one. An `EHR`, an `EHR_STATUS`, a `DIRECTORY`, tags and revision
  history hold no category.
- Whatever the map cannot classify exactly is recorded
  `ehds-unclassified`, with the reason (`unmapped`, `unbound` for a query
  reading a class bound to no id, `named-nothing`, `format-not-read` for a
  write in a simplified format, and so on) and the ids as evidence. A key
  matches only exactly: an id that differs by case, a space or a
  specialisation is unmapped. An unclassified access is answered as any
  other; it is never refused for it.

The `ehds-categories` entity's `detail` entries are `ehds-category`,
`ehds-category-basis` (`<category>:returned`, `written` or `queried`),
`ehds-no-category`, `ehds-unclassified`, `template-id`, `archetype-id`,
`version-uid`, `unmapped-id`, `category-map-digest` (the SHA-256 of the
map's canonical text, so a reader knows which map classified the record),
`delivered` and `stored-query`. The categories of one origin are named in
its `origin` entity when that origin alone contributed what the access
delivered.

### Failing closed

An access whose record cannot be stored is refused: the gateway stores the
record before the answer leaves, and when it cannot, it answers `503
access-unrecorded` and none of the data
([Errors](../integrate/errors.md)). A write the node took before the record
failed stays at the node; read the resource before writing again. An
answer of a patient-data operation that carries data and no record is
refused the same way, so no path of the gateway can hand out data without
its record.

### What stays out of the log

No record content reaches a node, the operator log, a span or a metric
label (§5.4, N33). The `log` destination writes the record's pattern,
action, outcome and the count of its entities, never the patient, the
caller, a template or archetype id or the client, so it is accepted for
the access log under the development profile alone: a production gateway
with a registry sends its access records to your Audit Record Repository
with `destination = "repository"`. A failure to store a
record is logged under the gateway's request id with the error chain, which
names no value.

### `[access_log]`

```toml
[access_log]
# national categories your national law adds (Art 14(1) third subparagraph)
national_categories = ["nl-example"]

[access_log.templates]
"Example Lab Report.v1" = ["medical-test-result"]
"Example Discharge.v1" = ["discharge-report"]
"Example Admin Note.v1" = "none"

[access_log.archetypes]
"openEHR-EHR-OBSERVATION.laboratory_test_result.v1" = ["medical-test-result"]
```

Each key is a template id or an archetype id, written exactly as the
`archetype_details` of your compositions write it. Each value is a list of
category codes, at least one, or `"none"`, your statement that the data
under the key belong to no category. A code that names no category, an
empty list, and a national code that repeats a priority category's code
are refused when the configuration loads. A change to `[access_log]` is
applied by a reload. Without a map every access is recorded unclassified,
so author one from the templates your members hold
(`GET {base}/v1/definition/template/adl1.4` at each member).

### The Annex II 3.2 checklist

| Item | Requirement | How FerroFED meets it |
|---|---|---|
| 3.2 | a record "on every access event or group of events" | one record per federated query, stored-query execution, routed read and routed write that reached a node, the console's included; stored before the answer leaves |
| 3.2(a) | the healthcare provider or other individuals who accessed the data | the provider agent and the Application agent of each record |
| 3.2(b) | the specific natural person or persons who accessed the data | the `agent:user`, from the token the gateway verified |
| 3.2(c) | the categories of data accessed | the `ehds-categories` entity, classified by your `[access_log]` map, `unclassified` with its evidence where the map cannot tell |
| 3.2(d) | the time and date of access | `recorded` |
| 3.2(e) | the origin or origins of the data | one `origin` entity per endpoint asked, with its node and outcome |
| 3.3 | tools to review and analyse the log data, or the connection of external software | the records go to your ATNA Audit Record Repository; FerroFED's own review interface is planned |
| 3.4 | retention periods and access rights by origin and category | planned: the records carry the origin and the category each needs |

## `[audit]`

```toml
[audit]
destination = "repository"             # "log" or "off" under development, or without a registry

[audit.repository]
url = "https://arr.example.org/fhir"   # the repository's FHIR base
hostname = "gateway.example.org"       # the gateway's network address in each record
spool_dir = "/var/lib/ferrofed/audit-feed-spool"
# source_id = "gateway.example.org"    # source.observer, the hostname by default
# enterprise_site = "2.999.40"         # source.site
# spool_max_bytes = 67108864
# spool_max_events = 100000
# spool_write_timeout_ms = 2000        # storing one record in the spool
# timeout_ms = 5000                    # each delivery
# retry_max_ms = 60000                 # the longest wait between two attempts
client_identity_file = "/run/secrets/atna-client.pem"  # when the repository asks
trust_roots_file = "/etc/ferrofed/atna-roots.pem"      # optional
```

- `destination` has no default outside `profile = "development"` once a
  registry is configured, or `[pixm]`, `[pdqm]` or `[pmir]` is set, since
  every access to patient data is recorded: `config check`, `serve` and a
  reload refuse the configuration without it, naming `audit.destination`.
  A build without the IHE binding records no access, and refuses a
  registry outside development. `off` is refused outside development; under
  development an unset `destination` records nothing.
- `log` writes each record as a structured event at the `ferrofed::audit`
  log target: the profile, the subtypes, the action, the outcome, the other
  party, and how many patient, query and resource entities the record
  holds. It never writes a patient identifier, a patient reference, the
  caller or the request, which may name one. An access record written
  there names no one, so outside development a gateway with a registry
  refuses `log`: `config check`, `serve` and a reload name
  `audit.destination` and ask for `repository` (Regulation (EU) 2025/327
  Annex II 3.2). `log` stays accepted under development, and outside it
  for a gateway with no registry, whose PIXm, PDQm or PMIR records it
  writes.
- `repository` posts each record to `[url]/AuditEvent` as FHIR JSON. The
  url is `https` outside development; under development plain `http` is
  accepted and named on the banner, and without `spool_dir` the spool is
  held in memory, which a restart loses.
- A change to `[audit]` takes a restart, as `[xcpd.audit_repository]` does:
  one forwarder drains each spool.

## The spool and the failure policy

The FHIR Feed records follow the policy of the ITI-55 audit trail. Every
record is written to the spool first, flushed to disk, and delivered from
there in order, so it counts as recorded once it is on disk. While the
repository is down, or answers `5xx`, `408` or `429`, the transactions go on
and the records wait in the spool, and the gateway tries again after a wait
that doubles from 250 ms up to `retry_max_ms`, with jitter. A record the
repository refuses with any other `4xx` would be refused again, so it is
moved to the spool's `quarantine` subdirectory, logged with its sequence
number and status and never its content, and the drain goes on; a spooled
file that is no FHIR `AuditEvent` is quarantined the same way. A quarantined
record stays counted under the spool's bounds until you remove it.

Only a record the gateway can neither deliver nor store is an audit
failure: a full spool (`spool_max_events` or `spool_max_bytes`), one that
cannot be written, or one that does not store the record in time
([A slow disk](#a-slow-disk)). The transaction then fails closed, and its
answer is never used:

| Transaction | When its record cannot be stored |
|---|---|
| An access to patient data | the answer is withheld: `503 access-unrecorded` with none of the data ([The access log](#failing-closed)) |
| ITI-83 | the member's resolution is unavailable: no member is asked, and the query fails `424` under all-or-nothing completeness |
| ITI-90, ITI-91 | the directory read fails as a directory that did not answer: the boot is refused, or a refresh keeps the registry in place |
| ITI-93 | the message is answered `503` and nothing is applied, so the Registry sends it again |
| ITI-94 | the exchange fails, and `GET /health/dependencies` reports the Registry `failing` with the fault `audit-failed` |

## A slow disk

Storing a record flushes it to the device twice, once for the file and once
for its directory, and the transaction waits for that before it uses its
answer. A disk that is slow or has stalled could hold the transaction
without limit, so the wait is bounded, on both spools:

- by `spool_write_timeout_ms` (2000 by default), in `[audit.repository]`
  and in `[xcpd.audit_repository]`, the longest one record may take to be
  stored, the wait for the write before it included;
- and, for an ITI-83 query, an ITI-78 search or ITI-119 match of the
  `[pdqm]` step, or an ITI-55 discovery, by the time the transaction was
  given: what is left of the patient query's budget, of the step's
  `timeout_ms`, or of the localization's time. Whichever of the two bounds
  comes first applies.

A record that `spool_write_timeout_ms` cuts off is an audit failure, as a
full spool is, with the outcomes in the table above, and an ITI-55
discovery fails closed under every `on_failure` policy
([The audit repository](localization.md#the-audit-repository)). A record still
being stored when its transaction's time runs out fails a transaction that
succeeded the same way: an ITI-55 discovery or a `[pdqm]` exchange then
fails closed under every `on_failure` policy, never widened to ask-all. The
gateway gives the localizer and the `[pdqm]` step a deadline a tenth of
their time short of its own, so a record cut off at that deadline reaches it
as the audit failure it is, before the gateway stops waiting. A transaction
that failed already, such as a PIX Manager or a responding gateway that did
not answer, reports its own failure instead, which the late record would
only hide. Either way the query never waits on the disk past its budget.

The write that missed its bound is not abandoned. It runs on until the disk
answers. Once it stores the record, the record is delivered like any other,
and the gateway logs a warning with the record's sequence number; a write
that fails in the end is logged as an error. Neither log line carries the
record. The spool's depth counts the record only once the write has stored
it. A transaction that failed this way may therefore still appear in the
repository, as the transaction it was: the gateway made or received it, and
ITI-20 has every stored record sent (ITI TF-2 §3.20.4.1.1).

One write runs at a time. While a write is stalled, every record behind it
waits for its turn and its transaction is answered at its own bound, so a
stalled disk holds one thread of the gateway, never one per transaction.
A record still waiting when its transaction stopped waiting stays queued:
it is written once the writes ahead of it end, and delivered like any
other, in the order the records were queued. A queued record holds its
place under `spool_max_events` and `spool_max_bytes` as a stored one does,
so a stalled disk holds no more records in memory than the spool could
hold on disk. A record past either bound is refused at once, without
waiting: its transaction fails as with any full spool, the refusal is
logged with its count and never the record, and
`ferrofed_audit_refused_total` counts it ([Metrics](metrics.md)). The mCSD directory
reads and the PMIR subscription exchanges run outside any patient query,
so `spool_write_timeout_ms` alone bounds their records.

The spool holds records that name patients. The gateway creates the
directory readable by its own user alone (`0700`) and every file `0600`,
and refuses to start when the directory gives its group or other users any
access. Put it on an encrypted volume. A spool belongs to one repository
configuration: the FHIR Feed spool and the ITI-55 syslog spool are two
directories.

`GET /health/dependencies` reports the FHIR Feed repository as
`audit_feed`: `up`, `degraded` while the gateway retries a failed delivery,
while records wait in the spool and while any sits in quarantine, and
`unknown` before the first record. The audit metrics of
[Metrics](metrics.md) count every spool, the FHIR Feed's included.
