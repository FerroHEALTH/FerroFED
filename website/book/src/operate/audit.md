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

## The address a request came from

[The access log](#the-access-log) records the address each request came from,
so it says from where a caller reached patient data. The gateway takes that
address from the connection, so behind a reverse proxy it is the proxy's,
`127.0.0.1` for a proxy on the same host. The client's address reaches the
gateway only in the `Forwarded` or `X-Forwarded-For` header the proxy
writes, and any client can write those headers too, so the gateway reads
them only from a proxy listed in `server.trusted_proxies` (RFC 7239 §8.1).
With the proxy listed, a request is named by the client the proxy names;
with none listed, the default, every request is named by its peer, and a
forwarded header changes nothing
([Behind a reverse proxy](public-address.md#behind-a-reverse-proxy)).

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
| (b) the specific natural person who accessed the data | the `agent:user` (`IRCP`): the token's `iss` and `sub`, the professional the token names, the assurance level of the authentication, who acts, `altId` the professional identification the requester claims state, and every purpose of use: see [The person behind an access](#the-person-behind-an-access) |
| (c) the categories of the data accessed | the entity named `ehds-categories`: see [Categories](#categories) |
| (d) the time and date | `recorded` |
| (e) the origin or origins of the data | one entity named `origin` per endpoint the access was sent to: the endpoint id, its node, its `system_id`, how it answered (`active`, `node-error`, `time-out`, and so on, or the node's HTTP status for a routed request) and the rows it sent |

Beside them, a record names the data subject: the patient by the
identifier and namespace the request named (`entity:patient`, as the ITI-83
record names it) or, for a request that named none, the patient the
identity binding holds under the `ehr_id` it reached
([The patient behind an `ehr_id`](#the-patient-behind-an-ehr_id)), and one
entity named `ehr` per `ehr_id` the access reached, with its endpoint and
how its patient was looked up. It carries the request as `entity:query`, the
gateway's request id as `entity:transaction` (the id the request log names
too), the address the request came from, and the outcome.

### The person behind an access

The `agent:user` carries what client authentication verified about the
natural person (Annex II 3.1, 3.2(b); [Professionals and
assurance](authentication.md#professionals-and-assurance)),
and no other part of the record carries it:

| From the verified token | In the `agent:user` |
|---|---|
| the IHE IUA `subject_name` | `who.display`, as BALP 1.1.4 §3:5.7.5.4 maps it |
| the IHE IUA `national_provider_identifier` | an `ihe-otherId` extension, its `valueIdentifier` typed `NPI` (HL7 v2 table 0203), as BALP 1.1.4 §3:5.7.5.4 maps it |
| the assurance level, `low`, `substantial` or `high` (Regulation (EU) No 910/2014 Art 8(2)) | an `ihe-assuranceLevel` extension, its `valueCodeableConcept` coded with the level's name |
| who acts: `person` when the token's `sub` names a natural person, `client` when a client application acts for the professional the token names | a `role` coded `person` or `client` |

The assurance level is recorded only when the issuer's
`[auth.issuer.assurance]` table declares the claim and the token states a
value it maps; the gateway never infers one, so a record of an issuer that
declares no mapping carries no level. A client is admitted to patient data
only when its issuer declares that its client tokens act for the
professional they name, and the record then names that professional in the
`agent:user` and the client in the Application agent. BALP fixes no
vocabulary for the assurance level and names no element for who acts, so
both codes, written with no `system`, are FerroFED's own design.

### A request a national contact point relays

A national contact point's connector acts for the professional of another
Member State its token names ([National contact
points](authentication.md#national-contact-points)). Its record names the
foreign provider as the provider agent, the foreign professional in the
`agent:user`, and the connector as the Application agent, the client that
relayed the request. Beside them, one entity named `ehds-relayed` carries
every attribute of Implementing Regulation (EU) 2026/2099 Annex Tables 1
and 2 the contact point asserted, each a `detail`:

| `detail` | Annex attribute |
|---|---|
| `contact-point` | the contact point that asserted them, by its issuer |
| `asserted` | `true`: the values are the contact point's assertion, never verified by the gateway |
| `country-code` | Table 1 `country_code` |
| `hp-family-name`, `hp-given-name` | Table 1 `family_name`, `given_name` |
| `hp-identifier`, `hp-issuing-authority` | Table 1 `hp_identifier` and its `issuing_authority_name` |
| `hp-professional-role` | Table 1 `hp_professional_role`, once per role, `system\|code` |
| `provider-identifier`, `provider-issuing-authority` | Table 2 `healthcare_provider_identifier` and its `issuing_authority_name` |
| `provider-name`, `provider-address` | Table 2 `healthcare_provider_name`, `healthcare_provider_address` |

A correlation identifier the connector sent in the header its issuer
declares is a `correlation-id` detail of `entity:transaction`, so the
record can be joined with the contact point's own exchange log. No log line
carries any of them. The entity and detail names are FerroFED's own design;
no text the gateway reads asks the national side for an audit format.

### Emergency access

Regulation (EU) 2025/327 lets a healthcare provider or health professional
be granted access to data a person restricted, "where necessary in order to
protect the vital interests of the data subject", and asks that "such cases
shall be logged in a clear and understandable format and shall be easily
accessible for the data subject" (Art 11(5); Art 8 grants the restriction).

The caller asserts an emergency access through the purpose of use its
token declares ([Purpose of use](authentication.md#purpose-of-use)), and
you name the codes that assert one in `[[access_log.emergency_purpose]]`.
A record whose token declares one of them carries one more entity:

| Element | Value |
|---|---|
| `type` | `4` (other), as the other entities FerroFED adds |
| `name` | `ehds-emergency-access` |
| `description` | a sentence that states the mark in words, citing Art 11(5) |
| `detail` `ehds-emergency-access` | `true` |
| `detail` `ehds-emergency-purpose` | each declared purpose that marked it, `system\|code` |

The purposes stay where BALP puts every purpose of use, in the
`agent:user` `purposeOfUse`, so the record keeps its BALP pattern. The
entity lets a person's access service show the mark without knowing which
codes your deployment maps.

What the mark does and does not say:

- It records what the caller asserted. The gateway reads it from the
  verified token's purposes alone, matching the code and its system
  exactly, and never infers it from the query, the data or a node's answer.
- It changes nothing else. The purpose reaches every node in the
  `openEHR-federation-client` token as any purpose does
  ([What a node is told about the caller](authentication.md#what-a-node-is-told-about-the-caller)),
  and the node decides whether to release restricted data (Federation Tier
  §13, N26). The gateway sends the same request it would send without the
  mark, asks no node again, and answers a node's refusal as it would
  without the mark. A refused or failed emergency access is marked too.
- It does not say that restricted data were reached. Consent and
  restrictions stay with the node, and Art 8 keeps the fact of a
  restriction from healthcare providers, so the gateway cannot tell. The
  node that released restricted data records that in its own log.
- The optional consent pre-filter is asked as configured, whatever the
  caller's purpose ([Consent](consent.md)); a member it drops as
  `consent-denied` is not asked, under an emergency purpose too.

For each marked access the gateway writes one `warn` line, "the access was
declared an emergency access by its purpose of use", under its request id
and with no other value, so your security monitoring can watch for it.
The person learns of the access through the access service of your Member
State, which reads the record with ITI-81 ([Reading the
log](#reading-the-log)). Art 9(1) gives the person information "including
through automatic notifications" on any access, "including access provided
in accordance with Article 11(5)"; that notification is the access
service's, which can search the repository for records naming the person
and the `ehds-emergency-access` entity. The gateway sends no notification
of its own.

FerroFED names no emergency purpose by default, so no access is marked
until you declare the codes your issuers use, and `config check` notes a
configuration that declares none. The HL7 v3 `ActReason` code system
(<https://terminology.hl7.org/CodeSystem-v3-ActReason.html>) defines two
candidates:

| Code | Display | Definition, in part |
|---|---|---|
| `BTG` | break the glass | policy override operations for "immediately needed health care for an emergent condition", which "may include override of subject of care consent directive restricting access" |
| `ETREAT` | Emergency Treatment | operations "for provision of immediately needed health care for an emergent condition" |

`BTG` names the override Art 11(5) describes. `ETREAT` is broader, and you
map it only when your national rules treat every emergency treatment as an
access in the vital interests. The IHE IUA example token declares `BTG`
beside `TREAT` (ITI TF-2 3.71.4.2.2.1.1).

### Categories

A record names the categories of the data accessed: the six priority
categories of Art 14(1) and the national categories you declare. Each is a
coded value, a system and a code in it, as a FHIR `Coding` is. The six
priority categories carry the codes of HL7 Europe's
`EEHRxFDocumentPriorityCategoryCS`, in the system
`http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs`
at version `1.0.0-ballot` (the EU Health Data API, `hl7.fhir.eu.health-data-api`
1.0.0-ballot), whose displays are the Art 14(1) terms. That code system is
the only one any EU artefact publishes for the categories, and the HL7
Europe Imaging Report guide requires its code on every imaging report. No
adopted act fixes how a log writes a category, so FerroFED writes these
codes until the common specifications of Art 36(1) say otherwise.

| Art 14(1) | Category | Code |
|---|---|---|
| (a) | patient summaries | `Patient-Summaries` |
| (b) | electronic prescriptions | `Electronic-Prescriptions` |
| (c) | electronic dispensations | `Electronic-Dispensations` |
| (d) | medical imaging studies and related imaging reports | `Medical-Imaging` |
| (e) | medical test results, including laboratory and other diagnostic results and related reports | `Laboratory-Reports` |
| (f) | discharge reports | `Discharge-Reports` |

The codes are case-sensitive, as the code system declares. In the map and
the retention table you name a priority category by its bare code, or by
`<system>|<code>`. A national category (Art 14(1) third subparagraph) is
the code your Member State's code system defines, in that system: you
declare it as `<system>|<code>` in `national_categories`, where the system
is the absolute URI of the national code system (or a URI you control,
until your Member State publishes one), and you name it the same way
everywhere else. Its code is held to the FHIR `code` rules only: no
leading, trailing or repeated whitespace. A record writes every category
as `<system>|<code>`, the FHIR search token form.

Neither AQL nor ITS-REST carries a category, so the gateway
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
  archetype predicates in `FROM` and in the paths it selects, and `=` on
  `archetype_node_id`, `archetype_details/archetype_id/value` and
  `archetype_details/template_id/value` in the top-level `AND` chain of
  `WHERE`, read from the syntax tree, never from a comment, a string
  compared with another path, `NOT`, `!=` or `NOT CONTAINS`. A selected
  path that names its archetype by a pattern or a parameter leaves the
  record `unbound`.
- An access spanning categories records all of them, and `none` never
  removes one. An `EHR`, an `EHR_STATUS`, a `DIRECTORY`, tags and revision
  history hold no category.
- Whatever the map cannot classify exactly is recorded
  `ehds-unclassified`, with the reason and the ids as evidence. The reasons
  are a closed set: `unmapped` (an id the map holds no key for), `unbound`
  (a query reading a class bound to no id), `named-nothing` (no root
  object and no id), `operation-not-read` (an operation whose data the
  gateway does not read), `format-not-read` (a body in a format it does
  not read, such as a simplified format), `body-not-read` (a body that
  holds no root object it can name) and `no-object-returned`. No
  category and unclassified are states of the record, never categories:
  neither is ever written as an `ehds-category`. A key
  matches only exactly: an id that differs by case, a space or a
  specialisation is unmapped. An unclassified access is answered as any
  other; it is never refused for it.

The `ehds-categories` entity's `detail` entries are `ehds-category`
(`<system>|<code>`), `ehds-category-basis` (`<system>|<code>:returned`,
`written`, `queried` or `construction`, the basis after the last `:`),
`ehds-category-version` (`<system>|<version>`, once per code system whose
version is known), `ehds-no-category`, `ehds-unclassified`, `template-id`,
`archetype-id`,
`version-uid`, `unmapped-id`, `category-map-digest` (the SHA-256 of the
map's canonical text, so a reader knows which map classified the record),
`delivered` and `stored-query`. Each origin that contributed what the
access delivered names its categories in its `origin` entity. When it alone
contributed, they are the access's own. When several did, each origin's
are classified from the rows it answered with, before the merge, so its
set can name a category of a row the merge then cut by `LIMIT` or
`DISTINCT`, and never misses one it delivered. An endpoint the query never
left the gateway for, such as one whose request waited out its deadline for
a slot of `federation.max_in_flight_per_node`, is no origin, and a query
that left the gateway for no endpoint writes no record.

### How long a record is kept

Regulation (EU) 2025/327 keeps the information on each access "available
for at least three years from each date of access" (Art 9(2)), and asks
for "different retention periods ... that take into account the origins and
categories" of the data (Annex II 3.4). The gateway holds no record, and
your Audit Record Repository "retains data according to local policies"
(IHE RESTful ATNA §3.81.4.1.3), so each record states the period it must be
kept for, and your repository's retention policy applies it.

You declare the periods in `[access_log.retention]`, in whole years: one
for every record, and one per category and per origin (an endpoint of the
registry). A record is kept for the longest period among the default, its
categories and its origins. Its origins are the endpoints its `origin`
entities name, every endpoint the access was sent to, one that answered
with nothing included. A record the category map could not classify
(`ehds-unclassified`) is kept for the longest period you declare anywhere,
since nothing says which of its parts it holds. Without the table every
record is kept three years. Taking the longest is FerroFED's own design: no
specification says how origin and category combine.

The `ehds-categories` entity carries three `detail` entries:

| `detail` | Value |
|---|---|
| `ehds-retention-years` | the period, in years |
| `ehds-retention-ends` | the first date, in UTC, on which the record may be deleted: the date of access plus the period, plus one day, so an access on 29 February keeps its full period |
| `ehds-retention-ground` | what called for the period: `default`, `unclassified`, `category:<system>\|<code>` or `origin:<endpoint>` |

Configure your repository to keep each record at least until its
`ehds-retention-ends`. A repository that cannot read a record's own
`detail` keeps every record for the longest period you declare, which meets
every record's period.

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

### Reading the log

The records are read at your Audit Record Repository, with ITI-81 Retrieve
ATNA Audit Event, the FHIR search an Audit Consumer runs against the
repository that received them (IHE RESTful ATNA §3.81). The gateway serves
no route of its own for the log and reads no record back. Annex II 3.3
asks for tools to review and analyse the log data "or" the connection of
external software for the same purpose: the repository and any ITI-81
Audit Consumer are that software. Choose a repository that supports the
Retrieve Audit Message Option (ITI TF-1 §9.2.3) at the FHIR base
`[audit.repository] url` names.

Every search names a period with `date` (§3.81.4.1.2.1), matched against
each record's `recorded`:

| Who reads | The search |
|---|---|
| A person, through your Member State's electronic health data access service (Art 9(2)) | `GET [base]/AuditEvent?date=ge2027-01-01&date=le2027-12-31&patient.identifier=<namespace>\|<identifier>`: the patient a request named, or the identity binding named behind the `ehr_id` it reached, is the record's `entity:patient`, whose `what.identifier` carries the namespace and the identifier |
| An operator, for one caller | `agent.identifier=<iss>\|<sub>`, the caller's issuer and subject |
| An operator, for one EHR at one member | `entity.identifier=\|<ehr_id>`, the record's `ehr` entity |
| An operator, for one member | `entity.identifier=\|<endpoint>`, the record's `origin` entity |
| An operator, for refused or failed accesses | `outcome=http://hl7.org/fhir/audit-event-outcome\|4,8,12` |

The repository answers with a FHIR `Bundle` of type `searchset` holding
the matching `AuditEvent`s (§3.81.4.2.2.2), which is also the documented
format to export the log in (Annex II 2.6): each record is written as
[What a record names](#what-a-record-names) lists. The repository returns
only the records "which the requester is authorized to view"
(§3.81.4.1.3), so the access rights of each reader are set there, and it
records every search as an `Audit Log Used` event of its own
(§3.81.5.1). FerroFED's tests run each search in the table above, and
the refusal of one with no `date`, against the records the gateway writes
to its harness repository, which answers ITI-81 and records each search
as that event. Neither ITI-81 nor the gateway gives a person's own
application a route to the log: the person reads it through the access
service their Member State provides (Art 9(2)).

#### The patient behind an `ehr_id`

A routed request addressed by `ehr_id`, such as a composition read or a
write under an EHR, and a query scoped to one `ehr_id`, name no patient.
Art 9(1) gives the person information on "any access", so the gateway
asks your identity binding which patient the member holds under that
`ehr_id`, in each namespace `[access_log] patient_namespaces` names, and
writes each identifier it finds as an `entity:patient`, as it writes a
patient the request named. The search by `patient.identifier` then finds
the access. Under the IHE binding the question is one ITI-83 to the
member's PIX Manager, the `ehr_id` in the member's domain as the source
identifier and each namespace's assigning authority as a target system
(PIXm 3.1.0 §2:3.83.4.1.2), recorded as every ITI-83 is. It is asked on
behalf of the caller, within `federation.per_node_timeout_ms`, of the
identity service alone: nothing is sent to a node, and no identifier
reaches a log line (§5.4, N33).

Each `ehr` entity says how its patient was looked up, in a `detail` named
`patient-lookup`:

| Value | Meaning |
|---|---|
| `request-named` | the request named the patient, who is the record's `entity:patient` |
| `found` | the identity service named the patient, written as an `entity:patient` |
| `not-found` | the identity service holds no identifier for the patient in a namespace asked |
| `unavailable` | the identity service failed or did not answer in time; the gateway logs the failure, with no identifier |
| `not-configured` | `patient_namespaces` is empty |
| `unsupported` | the identity binding cannot name a patient by an `ehr_id` |

The access is recorded and answered whatever the lookup says. Where the
patient is not named, the `ehr` entity still carries the `ehr_id`: search
the record with `entity.identifier=|<ehr_id>` for each `ehr_id` the person
holds at each member, which your identity service lists (for PIXm, an
ITI-83 with the person's identifier as the source and each member's
`ehr_id` domain as a target). A record that names more than one patient,
such as one identifier in each of two namespaces, writes each and claims
the plain BALP pattern, whose slices bound no patient.

A query over many patients' data, one that names no patient and is scoped
to no `ehr_id`, records neither: the gateway does not know whose rows it
returned, so a search by the person's identifier does not find it.

Labelling each record so a repository can limit access to it by category
and origin is planned ([#797](https://github.com/FerroHEALTH/FerroFED/issues/797)).

### `[access_log]`

```toml
[access_log]
# national categories your national law adds (Art 14(1) third subparagraph),
# each <system>|<code>
national_categories = ["https://example.org/fhir/CodeSystem/national-category|nl-example"]
# the namespaces the patient behind an ehr_id is named in (Art 9(1))
patient_namespaces = ["urn:oid:2.999.1"]

[access_log.templates]
"Example Lab Report.v1" = ["Laboratory-Reports"]
"Example Discharge.v1" = ["Discharge-Reports"]
"Example Admin Note.v1" = "none"

[access_log.archetypes]
"openEHR-EHR-OBSERVATION.laboratory_test_result.v1" = ["Laboratory-Reports"]

[access_log.retention]
years = 5                      # every record, at least 3 (Art 9(2)); 3 when unset

[access_log.retention.categories]
"Discharge-Reports" = 20       # an Art 14(1) code, or a national <system>|<code> declared above

[access_log.retention.origins]
"node-a" = 15                  # an endpoint id of the registry

[[access_log.emergency_purpose]]
system = "http://terminology.hl7.org/CodeSystem/v3-ActReason"
code = "BTG"                   # break the glass marks an emergency access (Art 11(5))
```

`patient_namespaces` lists the namespaces your Member State's access
service searches the log by, written as your identity binding names them:
under the IHE binding a namespace that is an absolute URI, or one
`[pixm]` maps to an assigning authority. An empty namespace is refused
when the configuration loads. With none, a request addressed by `ehr_id`
names no patient, and its record says `not-configured`.

Each key is a template id or an archetype id, written exactly as the
`archetype_details` of your compositions write it. Each value is a list of
categories, at least one, or `"none"`, your statement that the data under
the key belong to no category. A category is a priority code
(`Laboratory-Reports`), the same as `<system>|<code>`, or a declared
national category as `<system>|<code>`. A category that names no
category, such as a code in another letter case or a national code without
its system, an empty list, a national category without a system, with a
system that is no absolute URI, in the priority categories' system, or
with a code that is no FHIR `code`, are refused when the configuration
loads. The lower-case spellings of the development builds before the
codes were settled (`patient-summary`, `medical-test-result` and the
rest) are refused; the upgrade notes give the new code of each. A change to `[access_log]` is
applied by a reload. Without a map every access is recorded unclassified,
so author one from the templates your members hold
(`GET {base}/v1/definition/template/adl1.4` at each member).

Each period under `[access_log.retention]` is a whole number of years, at
least 3. A smaller one, a category code that names no category, and an
origin that is no endpoint of the registry are refused when the
configuration loads, naming the key. A change is applied by a reload and
reaches the records written after it.

Each `[[access_log.emergency_purpose]]` needs a `code`, and takes a
`system`. Without a `system` it matches only a purpose a token declares
with no system. An empty code or system and an unknown key are refused when
the configuration loads ([Emergency access](#emergency-access)).

### The Annex II 3.2 checklist

| Item | Requirement | How FerroFED meets it |
|---|---|---|
| 3.2 | a record "on every access event or group of events" | one record per federated query, stored-query execution, routed read and routed write that reached a node, the console's included; stored before the answer leaves |
| 3.2(a) | the healthcare provider or other individuals who accessed the data | the provider agent and the Application agent of each record |
| 3.2(b) | the specific natural person or persons who accessed the data | the `agent:user`, from the token the gateway verified: the professional's name and identifier, the assurance level when one was established, and whether a person or a client acted |
| 3.2(c) | the categories of data accessed | the `ehds-categories` entity, classified by your `[access_log]` map, `unclassified` with its evidence where the map cannot tell |
| 3.2(d) | the time and date of access | `recorded` |
| 3.2(e) | the origin or origins of the data | one `origin` entity per endpoint the query was sent to, with its node, its outcome and its categories |
| Art 11(5) | an access to restricted data in the vital interests "logged in a clear and understandable format" | the `ehds-emergency-access` entity of a record whose token declares a purpose you name: see [Emergency access](#emergency-access) |
| 3.3 | tools to review and analyse the log data, or the connection of external software | the records go to your ATNA Audit Record Repository and are read there with ITI-81 by any Audit Consumer, your Member State's access service included: see [Reading the log](#reading-the-log) |
| 3.4 | retention periods and access rights by origin and category | each record states the period its categories and origins call for, never under three years (Art 9(2)): see [How long a record is kept](#how-long-a-record-is-kept); access rights are set at your repository, and labelling the records for them is planned ([#797](https://github.com/FerroHEALTH/FerroFED/issues/797)) |

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
| ITI-94 | the exchange fails, and `GET {base}/operator/dependencies` reports the Registry `failing` with the fault `audit-failed` |

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

`GET {base}/operator/dependencies` reports the FHIR Feed repository as
`audit_feed`: `up`, `degraded` while the gateway retries a failed delivery,
while records wait in the spool and while any sits in quarantine, and
`unknown` before the first record. The audit metrics of
[Metrics](metrics.md) count every spool, the FHIR Feed's included.
