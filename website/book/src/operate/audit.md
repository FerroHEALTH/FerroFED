<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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

## `[audit]`

```toml
[audit]
destination = "repository"             # or "log", or "off" under development

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

- `destination` has no default outside `profile = "development"` once
  `[pixm]`, `[pdqm]`, `[registry.mcsd]` or `[pmir]` is set: `config check`, `serve`
  and a reload refuse the configuration without it, naming
  `audit.destination`. `off` is refused outside development; under
  development an unset `destination` records nothing.
- `log` writes each record as a structured event at the `ferrofed::audit`
  log target: the profile, the subtypes, the action, the outcome, the other
  party, and how many patient, query and resource entities the record
  holds. It never writes a patient identifier, a patient reference or the
  request, which may name one.
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
