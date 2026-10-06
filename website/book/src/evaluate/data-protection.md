<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Data protection

This page is for the people who write a deployment's data protection impact
assessment (DPIA) and its records of processing. It lists the personal data a
FerroFED gateway processes, where each item is held, how long it is kept and
what protects it. It then states the roles of a deployment, what the gateway
offers an entity under the NIS2 Directive, and what the documents claim about
the medical device regulation.

The legal texts are cited from the Official Journal:

- Regulation (EU) 2016/679, the General Data Protection Regulation (GDPR),
  <http://data.europa.eu/eli/reg/2016/679/oj>;
- Regulation (EU) 2025/327, the European Health Data Space Regulation (EHDS),
  vendored at
  [`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml);
- Directive (EU) 2022/2555, the NIS2 Directive,
  <http://data.europa.eu/eli/dir/2022/2555/oj>;
- Regulation (EU) 2017/745 on medical devices (MDR),
  <http://data.europa.eu/eli/reg/2017/745/oj>.

This page is not legal advice. It describes what the software does, so that
a deployment's counsel and data protection officer can decide what that means
for the deployment. [The last section](#what-counsel-must-confirm) lists what
they must confirm.

## What the gateway holds

FerroFED holds no clinical data of its own. It passes each member CDR's
answer to the client, merges the answers, and forwards a write to the CDR
that owns the record. Clinical content is in the gateway's memory for the
length of one request and is never written to disk or to a log.

What it does keep is routing state, audit records and operational telemetry.
Several of these hold personal data: an `ehr_id` is a pseudonymous key to one
patient's record at one node, the caller's identity names a health
professional, the IHE audit records name the patient, and the access records
name both. Every row of the inventory below names what it carries.

## Processing inventory

| Store or channel | Personal data it carries | Where | How long | Protection | Settings |
|---|---|---|---|---|---|
| The client's request | the patient identifier in the AQL or a parameter; the caller's access token; for a write, the clinical content | process memory | one request, at most `server.request_timeout_ms` (30 s) | the identifier is held as a `PatientRef` whose `Debug` and `Display` are redacted, never serialised, logged, traced or measured; the token is verified before anything else is read ([Client authentication](../operate/authentication.md)) | `server.request_timeout_ms`, `server.body_limit_bytes` |
| Requests to the identity services | the patient identifier, sent to the PIX Manager (ITI-83), the PDQm Supplier (ITI-78 or ITI-119), each XCPD responding gateway (ITI-55) and the NVI (a pseudonymised BSN); to Mitz, the patient identifier with the professional's UZI number and role and the organisation's URA and type | in transit to each service you configure | one exchange | `https` required outside the development profile ([What must travel encrypted](../operate/configuration.md#what-must-travel-encrypted)); optional mutual TLS | `[pixm]`, `[pdqm]`, `[xcpd]`, `[nl_gf.nvi]`, `[nl_gf.mitz]` |
| Requests to the member nodes | the node's own `ehr_id`; the caller's `sub`, the issuer that vouched for it, the caller's organisation, purpose of use and scopes, in the signed `openEHR-federation-client` token; for a write, the clinical content byte for byte | in transit to each member asked | one request | no patient identifier in any part the gateway composes (§5.4.1, N33), checked by the outbound gate ([Where the patient identifier stops](../how-it-works/identifier-hygiene.md)); the caller's own token is never forwarded | `[credentials."<id>"]`, `[signing]` |
| The answers | clinical rows and documents from each member | process memory, then the client | one request | each node decides what it releases (§13.2, N26, N27); a node error quoted in `meta.federation` has each consumed value masked | none |
| Resolution bindings | the caller (issuer, subject, client) with the `{node, ehr_id}` pairs its resolutions found | process memory, per replica | `federation.binding_ttl_ms` after the last resolution that returned the pair (15 minutes by default); lost on restart | keyed by `ehr_id`, never by a patient identifier; seen by no other caller; bounded by `binding_capacity` (100 000) | [Resolution bindings](../operate/registry.md#resolution-bindings) |
| The `ehr_id` index | which member holds which `ehr_id` | process memory, per replica | least recently used past `ehr_index_capacity` (100 000); lost on restart | no caller and no patient identifier | [The `ehr_id` index](../operate/registry.md#the-ehr_id-index) |
| Integrity incidents | an `ehr_id`, only when it is a bare UUID, and the endpoints that claim it | process memory (the last 25 of each kind) and one log line | until restart; the log line as long as your log pipeline keeps it | no patient identifier; read on the operator surface with the operator scope ([Integrity incidents](../operate/registry.md#integrity-incidents)) | none |
| Per-caller rate buckets | the caller's issuer and `client_id` | process memory, per replica | until the bucket refills; at most 10 000 callers | none read outside the process | `[server.caller_rate]` |
| IHE audit spools | the patient identifier inside each ITI-83, ITI-78, ITI-119 and ITI-55 record; the identities a PMIR message names; the caller's `iss`, `sub`, `client_id` and purposes of use on a record made for a caller | a directory on disk, one per replica | until the Audit Record Repository accepts the record; a quarantined record until you remove it | directory `0700`, files `0600`, refused at start when open to other users; bounded by `spool_max_bytes` (64 MiB) and `spool_max_events` (100 000); encryption at rest is the volume's ([The audit trail](../operate/audit.md)) | `[audit.repository]`, `[xcpd.audit_repository]` |
| The ATNA Audit Record Repository | the same records | your repository | your repository's retention | ITI-20 over TLS: the FHIR Feed over `https`, syslog over TLS | the repository's own |
| The access records | for each federated query, stored-query execution, routed read and routed write that reached a node: the caller's `iss`, `sub`, `client_id`, organisation, professional identification and purposes of use; the patient identifier and namespace the request named, and each `ehr_id` reached with its endpoint; the request as the client sent it, which may name the patient; the request id and the address the request came from; each endpoint asked with its node, `system_id`, outcome and row count; the categories of the data | the `[audit]` spool, then the Audit Record Repository | as the IHE audit records above | stored before the answer leaves, and an access whose record cannot be stored is answered `503 access-unrecorded` with none of the data; the spool and transport protections above; no part reaches a node, the log, a span or a metric label ([The access log](../operate/audit.md#the-access-log)) | `[audit]`, `[audit.repository]`, `[access_log]` |
| The `log` audit destination | the profile, the action, the outcome, the server of a request the gateway sent, counts of entities, and `on_behalf` as `caller` or `gateway` | the log | your log pipeline's retention | no patient identifier, no request content, no `sub`, `client_id` or issuer of the caller, and no client of a request the gateway received; refused outside development for a gateway with a registry, because it cannot hold the access records | `[audit] destination = "log"`, `[xcpd] audit = "log"` |
| The request log and every other log line | the method, route template, status, latency and request id; a security event's reason; in the edge mode, keyed HMAC references to the subject and client that change at every start | stdout, then your log pipeline | your log pipeline's retention | no request body, AQL text, header value, request path or patient identifier ([What the log records](../operate/configuration.md#what-the-log-records)) | `[telemetry] filter`, `format` |
| Metrics | none: every label value comes from a closed set or the registry | the admin listener, the OTLP push | your metrics store's retention | [Metrics](../operate/metrics.md) | `[metrics]` |
| Trace spans | none: route templates, registry ids, operations, statuses and counts; the trace id is the gateway's own, never the client's | your OpenTelemetry collector | your collector's retention | [Tracing](../operate/tracing.md) | `[telemetry] otlp_endpoint`, `trace_sample_ratio` |
| Stored-query definitions | none: parameterised AQL, and a definition that names a patient by a literal is refused | `redb`, PostgreSQL or read-only files | durable; a stored version is immutable (§12.7, N44) | the bound values of an invocation are never written ([Stored queries](../operate/queries-and-areas.md#stored-queries)) | `[stored_queries]` |
| The PMIR identity feed | the Patient Master Identities each ITI-93 message names, with their identifiers and demographics | process memory for one message, then the audit spool | one message; the audit record as above | `https` outside development; a feed token from a file; only the kind and count of each change is logged ([The identity feed](../operate/identity.md#the-identity-feed-pmir)) | `[pmir]` |
| The operator console's sessions | the operator's ID Token claims and access token | the console's process memory | `idle_timeout_s` (1 800) without a request, `absolute_timeout_s` (43 200) at most, and no longer than the access token; lost on restart | held on the server, known to the browser by an opaque `HttpOnly` cookie; every page `no-store`; a query answer is rendered and not kept ([The operator console](../operate/operator-console.md)) | `[session]`, `[oidc]` |

Two kinds of state hold no personal data: the registry document (organisations,
endpoints, `system_id` mappings) and the `creating_system_id` routes the
gateway learns. The signing key and the onward credentials are secrets: the
gateway shows each as `***` wherever it renders the configuration, and the
[hardening guide](../operate/hardening.md#secrets) has them read from files.

The gateway sends nothing to its manufacturer. Every outbound connection goes
to a URL your configuration names.

### The record of each access

The gateway records every access to patient data it intermediates: each
federated query and stored-query execution that a member was sent, and each
routed read and write a node acted on, the operator console's queries
included. Each record names the verified caller, the patient and every
`ehr_id` reached, each endpoint asked and the categories of the data. The
gateway stores it in the `[audit]` spool before the answer leaves, and
answers an access whose record cannot be stored with `503
access-unrecorded` and none of the data
([The access log](../operate/audit.md#the-access-log)). The record follows
Annex II, point 3.2, of the EHDS Regulation, and the library that builds it
is `crates/ehds-logging`.

What the record does not cover:

- A definition request, such as a template or a stored-query definition,
  reaches no patient data and is not recorded.
- Under the development profile, an unset `[audit] destination` records
  nothing, and `log` writes counts that name no one. Outside development, a
  gateway with a registry refuses every destination but `repository`.
- A build without the IHE binding records no access, and refuses a registry
  outside development.
- FerroFED offers no interface of its own to review the records (Annex II,
  point 3.3), and keeps them for no period by origin and category (point
  3.4). Both are planned
  ([#521](https://github.com/FerroHEALTH/FerroFED/issues/521)); until then
  your Audit Record Repository decides who reads the records and how long
  it keeps them.

Each member node still audits who asked: every request carries the verified
caller in the `openEHR-federation-client` token and the gateway's request
id (§13.1, N24;
[Verifying it at the node](../operate/authentication.md#verifying-it-at-the-node)).

## Retention

The gateway decides the retention of what it holds in memory. Everything it
hands to another system is kept as long as that system keeps it.

| Data | Default | Setting | What decides the rest |
|---|---|---|---|
| A resolution binding | 15 minutes after the last resolution that returned it | `federation.binding_ttl_ms`, `binding_capacity` | a restart forgets it |
| An `ehr_id` index entry | until 100 000 newer entries push it out | `federation.ehr_index_capacity` | a restart forgets it |
| An operator console session | 30 minutes idle, 12 hours at most | `session.idle_timeout_s`, `absolute_timeout_s` | the access token's own lifetime |
| An IHE audit record or an access record in the spool | until the repository accepts it | `spool_max_bytes`, `spool_max_events` | a quarantined record waits for you to remove it |
| An audit record at the repository | none from the gateway | none | the repository's retention policy |
| Log lines, metrics, spans | none from the gateway | none | your log, metrics and trace stores |
| Stored-query definitions | kept | none | they hold no personal data |

GDPR Art 5(1)(e) asks that personal data be "kept in a form which permits
identification of data subjects for no longer than is necessary". Set the
retention of the audit repository and of the log pipeline in your records of
processing. The EHDS Regulation asks that information on each access to a
person's data through the health professional access service "be available
for at least three years from each date of access" (Art 9(2)). That duty
belongs to the access service. The gateway's access records are kept as long
as your Audit Record Repository keeps them, so its retention policy decides
whether they meet that period; retention by origin and category is planned
([#521](https://github.com/FerroHEALTH/FerroFED/issues/521)).

## Roles

The EHDS Regulation takes the definitions of "controller" and "processor"
from GDPR Art 4(7) and (8) (EHDS Art 2(1)(a)). A controller "determines the
purposes and means of the processing of personal data"; a processor
"processes personal data on behalf of the controller". Two or more
controllers that "jointly determine the purposes and means of processing"
are joint controllers (GDPR Art 26(1)).

FerroFED is software. Its manufacturer receives no personal data from a
deployment, so it is neither the controller nor a processor of a
deployment's processing. Its obligations as the manufacturer of an EHR system
are those of the EHDS Regulation ([Regulatory status](regulatory-status.md),
decided on [#519](https://github.com/FerroHEALTH/FerroFED/issues/519)).
The operator of a deployment is the party the GDPR roles attach to.

### A hospital that runs its own gateway

A hospital that runs FerroFED over its own CDRs, or over the CDRs of a group
it belongs to, decides why patient data is federated and by what means. It is
the controller of the gateway's processing. Its hosting provider, if the
gateway runs at one, is a processor under a GDPR Art 28 contract. Each member
CDR's operator remains the controller of its own CDR and of what it releases.

### A regional operator that runs a gateway for several hospitals

A regional operator that runs one gateway for several healthcare providers
can stand in either of two roles, and the facts of the arrangement decide
which:

- **A processor for each hospital.** The hospitals decide the purposes, and
  the operator runs the gateway on their documented instructions. Each
  hospital then needs an Art 28(3) contract with the operator, and the
  operator keeps a record of processing per controller under Art 30(2).
- **A joint controller with the hospitals.** Where the operator and the
  hospitals together set the rules of the federation (who may ask, for which
  purposes, which members take part), they jointly determine the purposes
  and means, and Art 26 asks for an arrangement that sets out their
  responsibilities.

FerroFED's own design points to facts counsel will weigh: the operator
chooses the issuers it trusts, the members it admits, the identity services
it asks and the audit repository it writes to; each member node still makes
its own release and consent decision (§13.2, N26, N27).

### The other parties

| Party | What it does with personal data in a federation |
|---|---|
| Each member CDR's operator | holds the clinical record and decides what it releases to the gateway |
| The PIX Manager, PDQm Supplier, XCPD gateways, NVI and Mitz | receive the patient identifier, and Mitz the professional's identity, to answer the gateway |
| The caller's issuer | issues the token that names the professional |
| The Audit Record Repository | stores the IHE audit records and the access records, which name the patient and the professional |
| The log, metrics and trace stores | receive the operational telemetry above |

Each is a recipient in the sense of GDPR Art 30(1)(d) and has a role of its
own to settle.

## DPIA inputs

GDPR Art 35(3)(b) requires an impact assessment for "processing on a large
scale of special categories of data referred to in Article 9(1)", and data
concerning health is one. A gateway that federates a region's patient
records will usually meet that test. These are the facts about the gateway an
assessment needs; the deployment adds its own:

- **Nature.** Intermediation: the gateway resolves the patient through an
  identity service, sends each member a query scoped to that member's
  `ehr_id`, merges the answers and routes follow-up reads and writes. It
  stores no clinical data.
- **Scope.** Every category of clinical data the member CDRs hold, because
  the gateway selects nothing by category ([Regulatory status](regulatory-status.md#intended-purpose)).
  The patients are those the members hold; the professionals are the callers
  of the trusted issuers.
- **Purposes.** Patient care by healthcare providers. Each token declares a
  purpose of use, and a token without one is refused by default
  ([Purpose of use](../operate/authentication.md#purpose-of-use)).
- **Measures.** The [threat model](threat-model.md) names the risks the
  gateway mitigates, the setting or test behind each mitigation, and the
  risks the deployment carries. The [hardening guide](../operate/hardening.md)
  turns them into a checklist.
- **The risks to bring into the assessment.** The patient identifier leaving
  for the identity services; the caller's identity reaching every member
  asked; the audit spool on disk, which holds the access records until the
  repository accepts them; the access records at the repository, which name
  the patient and the professional for every access; and a wrong link at
  the identity service routing a request to another patient's EHR.

## A template for the record of processing

GDPR Art 30(1) lists what a controller's record holds. This table fills in
what FerroFED determines and leaves the rest to you. A processor's record
under Art 30(2) holds rows (a), (e) and (g), and "the categories of
processing carried out on behalf of each controller" in place of (b) to (d).

| Art 30(1) | What to write |
|---|---|
| (a) the controller, joint controller, representative and data protection officer | yours |
| (b) the purposes of the processing | patient care by healthcare providers: answering a professional's query from the federation's CDRs and routing the follow-up reads and writes; add the purposes your issuers' tokens declare |
| (c) the categories of data subjects and of personal data | patients of the member CDRs: patient identifiers, `ehr_id`s, and health data in transit; health professionals: the token's subject, issuer, client, organisation, scopes and purposes of use; operators: their console sign-in |
| (d) the categories of recipients | the member CDRs; the identity services; the Audit Record Repository; your log, metrics and trace stores ([The other parties](#the-other-parties)) |
| (e) transfers to a third country | none by the gateway; each URL in your configuration decides where data goes |
| (f) the time limits for erasure | [Retention](#retention), with your repository's and log pipeline's periods |
| (g) the technical and organisational measures | the [threat model](threat-model.md) and the [hardening guide](../operate/hardening.md), with your own controls |

## NIS2

NIS2 Annex I, sector 5 "Health", lists "Healthcare providers as defined in
Article 3, point (g), of Directive 2011/24/EU". The Directive applies to
such a provider that is at least a medium-sized enterprise (Art 2(1)), with
the exceptions of Art 2(2). One that exceeds the ceilings for medium-sized
enterprises is an essential entity (Art 3(1)(a)), and one that does not is
an important entity (Art 3(2)). A regional operator that runs systems for
hospitals may fall under sector 9, "ICT service management
(business-to-business)", whose "managed service provider" provides "services
related to the installation, management, operation or maintenance of ICT
products ... either on customers' premises or remotely" (Art 6(39)). Whether
either applies is for the entity to establish.

NIS2 puts the duties on the entity, never on its software. What FerroFED
offers an entity for them:

| NIS2 | What the gateway offers |
|---|---|
| Art 21(2)(b) incident handling | the security events under `ferrofed::security`, each counted in `ferrofed_security_events_total` by event and reason; integrity incidents under `ferrofed::integrity`; the shipped alert rules, among them `FerroFEDCallerRefusals`, `FerroFEDOutboundGateStopped`, `FerroFEDAuthenticationUnavailable` and `FerroFEDIntegrityIncident` ([Metrics](../operate/metrics.md#dashboard-and-alert-rules)); one request id shared by the client, the log and every node |
| Art 21(2)(c) business continuity | health and readiness probes, a bounded drain, several replicas behind one address ([Health probes](../operate/health.md), [Running several replicas](../operate/deployment-shape.md#running-several-replicas)); the stored-query store is the one durable store to back up |
| Art 21(2)(d) supply chain security | signed release provenance, a CycloneDX and an SPDX SBOM per binary, image attestations ([The container image](../operate/container.md#the-release-binaries)) |
| Art 21(2)(e) vulnerability handling and disclosure | private reporting through GitHub security advisories, with an acknowledgement within seven days ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md)) |
| Art 21(2)(h) cryptography | the transport rules that refuse a credential or a patient identifier over plain `http` outside development, and the trust anchors over `https` ([What must travel encrypted](../operate/configuration.md#what-must-travel-encrypted)); ES256 and ES384 signing keys with rotation |
| Art 21(2)(i) access control | every caller authenticated by token, SMART on openEHR scopes per route, an operator scope for the operator surface and the admin listener's write actions ([Client authentication](../operate/authentication.md)), and a scrape token or mutual TLS for the metrics off loopback ([Metrics](../operate/metrics.md#who-the-admin-listener-serves)) |
| Art 21(2)(j) multi-factor authentication | none in the gateway: the caller's issuer and the console's OpenID Provider authenticate the person |

Art 23(4) sets the reporting clock: an early warning "within 24 hours of
becoming aware of the significant incident", an incident notification within
72 hours, and a final report "not later than one month" after that
notification. The gateway reports nothing to a CSIRT or a competent
authority. It supplies the evidence the entity reports from: the alerts, the
security and integrity log lines, and the dependency states of
`GET {base}/health/dependencies`. A personal data breach has its own clock
under GDPR Art 33: the controller notifies "where feasible, not later than
72 hours", and a processor notifies the controller "without undue delay".

The EHDS Regulation has the manufacturer of an EHR system report a serious
incident "not later than three days" after becoming aware of it, "without
prejudice to incident notification requirements under Directive (EU)
2022/2555" (Art 44(7)). That is the manufacturer's duty, planned in
[#672](https://github.com/FerroHEALTH/FerroFED/issues/672).

## The medical device question

The documents classify FerroFED under the EHDS Regulation alone. They do not
claim that FerroFED is a medical device under Regulation (EU) 2017/745, no
FerroFED release carries a CE marking under it, and none of the documents
states an intended purpose in its terms.

MDR Art 2(1) defines a medical device as software, among other articles,
"intended by the manufacturer to be used ... for human beings for one or more
of the following specific medical purposes", such as "diagnosis, prevention,
monitoring, prediction, prognosis, treatment or alleviation of disease".
Recital 19 adds that software "specifically intended by the manufacturer to
be used for one or more of the medical purposes" qualifies, "while software
for general purposes, even when used in a healthcare setting ... is not a
medical device".

FerroFED's [intended purpose](regulatory-status.md#intended-purpose) is to
find a patient's records across a federation of CDRs and return them with
their origin, and to route follow-up reads and writes to the CDR that holds
the record. The reasoning a deployment's counsel should check is this:

- **What FerroFED does to the data.** It resolves a patient, rewrites the
  query per node, and merges the answers: `DISTINCT`, `ORDER BY`, paging and
  aggregates across nodes, and deduplication by version identity when the
  client asks for it (§10). It computes no score, flags no finding and
  recommends nothing. A row's values are the node's own, beside the endpoint
  they came from, except a selected subject, which the gateway puts back
  (N5), and an aggregate, which it combines from the members' answers.
- **What could still matter.** The merge decides which rows a clinician sees
  and in which order, and an aggregate is computed over the members'
  answers. A missing member is named in `meta.federation`, and by default a
  member that was asked and did not answer fails the whole query (§11.4).
  Counsel should
  weigh whether that selection is a medical purpose, against Art 2(1) and the
  qualification guidance the Medical Device Coordination Group publishes.
- **Where the EHDS Regulation points.** Recital 42 says "certain software
  components of EHR systems could be considered medical devices" and should
  then be certified under the MDR as well. Art 27 binds manufacturers of
  medical devices that "claim interoperability" with the harmonised
  components; FerroFED makes no such claim. Art 1(5) leaves the MDR
  untouched for "the security of medical devices ... that interact with EHR
  systems".
- **What a deployment adds.** A client application, a stored query an
  operator publishes, or a change to FerroFED's code is outside the
  manufacturer's intended purpose. Each can bring a medical purpose of its
  own, and is classified on its own.

If counsel finds that a deployment's software is a medical device, the MDR
classification rules decide its class; Annex VIII, Rule 11, covers "software
intended to provide information which is used to take decisions with
diagnosis or therapeutic purposes".

## What counsel must confirm

This page states facts about the software. It decides no deployment's
obligations. A deployment's counsel and data protection officer should at
least confirm:

1. Who is the controller of the gateway's processing, and whether a regional
   operator is a processor under GDPR Art 28 or a joint controller under
   Art 26.
2. The lawful basis under GDPR Art 6 and the condition under Art 9(2) for
   each purpose of use your issuers' tokens declare.
3. Whether the deployment needs a DPIA under Art 35, and its outcome.
4. The retention of the audit repository and the log pipeline, and the
   access rights to each.
5. Whether the deployment's operator is an essential or important entity
   under NIS2, and which national transposition applies.
6. That neither FerroFED nor anything the deployment adds to it is a medical
   device under Regulation (EU) 2017/745, or, where something is, its
   classification and conformity route.
7. The EHDS points listed on [Regulatory status](regulatory-status.md#not-legal-advice).
