<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The audit trail

Every IHE transaction the gateway makes or receives is audited as its
profile requires, to an ATNA Audit Record Repository over ITI-20 Record
Audit Event. Two configurations decide where the records go:

- `[xcpd] audit` and `[xcpd.audit_repository]` for the XCPD localizer's
  ITI-55 exchanges, which XCPD audits as a DICOM audit message over syslog
  ([The audit repository](identity.md#the-audit-repository));
- `[audit]` for the PIXm, mCSD and PMIR transactions, which each profile
  audits as a FHIR `AuditEvent` built on the IHE Basic Audit Log Patterns
  (BALP). BALP has the gateway send it over the ATX: FHIR Feed Option of
  ITI-20: a FHIR `create` at the repository's FHIR base (the RESTful ATNA
  supplement, ITI TF-2 §3.20.4.2).

## What each transaction records

| Transaction | When | Record | Patient named | Section |
|---|---|---|---|---|
| ITI-55 Cross Gateway Patient Discovery | each discovery the XCPD localizer makes | the Initiating Gateway's DICOM audit message, over syslog | inside the base64 query parameters | `[xcpd]` |
| ITI-83 Mobile Patient Identifier Cross-reference Query | each query the PIXm resolver or localizer asks a PIX Manager | PIXm Query Consumer audit (BALP Patient Query): the request as sent, base64 | the source identifier, system and value | `[audit]` |
| ITI-90 Find Matching Care Services | each search of the mCSD directory the registry is read from, one per resource type | mCSD Care Services Query audit (BALP Query): the first request of the search, base64 | none | `[audit]` |
| ITI-91 Request Care Services Updates | each history of a registry refresh, one per resource type | mCSD Care Services Updates audit: the first request, base64 | none | `[audit]` |
| ITI-93 Mobile Patient Identity Feed | each message the Patient Identity Registry sends to the feed route | PMIR Feed audit, the Registry as the source and the gateway's callback as the destination: the `MessageHeader` | one entity per Patient Master Identity the message names, by its `Patient/<id>` reference or an identifier | `[audit]` |
| ITI-94 Subscribe to Patient Updates | each subscription create, read and delete the identity feed makes | PMIR Subscription Create, Read and Delete audits (BALP Create, Read, Delete): the `Subscription` and, for a create, its criteria | none: a subscription names an identifier system at most | `[audit]` |
| The search of the Registry's subscriptions by callback | before each create | BALP Query: the request as sent, base64; ITI-94 defines no search | none | `[audit]` |

PDQm (ITI-78) is not used by the gateway yet (#487). The ITI-78 client of
`crates/ihe-iti` records the PDQm Query Consumer audit when it is given a
recorder, so the gateway records it from the day it asks a PDQm Supplier.

Every record names the gateway as `source.observer` and as its own agent,
by `source_id` (the hostname by default), at the network address
`hostname` gives, and names the other party by its FHIR base URL without
its userinfo or query. No record names a user agent, which every BALP pattern leaves optional.

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
# timeout_ms = 5000                    # each delivery
# retry_max_ms = 60000                 # the longest wait between two attempts
client_identity_file = "/run/secrets/atna-client.pem"  # when the repository asks
trust_roots_file = "/etc/ferrofed/atna-roots.pem"      # optional
```

- `destination` has no default outside `profile = "development"` once
  `[pixm]`, `[registry.mcsd]` or `[pmir]` is set: `config check`, `serve`
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
failure: a full spool (`spool_max_events` or `spool_max_bytes`) or one that
cannot be written. The transaction then fails closed, and its answer is
never used:

| Transaction | When its record cannot be stored |
|---|---|
| ITI-83 | the member's resolution is unavailable: no member is asked, and the query fails `424` under all-or-nothing completeness |
| ITI-90, ITI-91 | the directory read fails as a directory that did not answer: the boot is refused, or a refresh keeps the registry in place |
| ITI-93 | the message is answered `503` and nothing is applied, so the Registry sends it again |
| ITI-94 | the exchange fails, and `GET /health/dependencies` reports the Registry `failing` with the fault `audit-failed` |

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
