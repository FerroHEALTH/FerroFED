<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Instructions for use

These are FerroFED's instructions for use under Regulation (EU) 2025/327 on
the European Health Data Space (the EHDS Regulation). Art 30(1)(d) has every
EHR system accompanied, free of charge, by "clear and complete instructions
for use"; Art 32(2)(d) has them cover "its maintenance, in accessible
formats"; and Annex III, point 1(j), puts "the instructions for use for the
user and, where applicable, installation instructions" in the technical
documentation. Annex II, point 1.2, asks that the system "can be supplied
and installed, taking into account the instructions and information provided
by the manufacturer, without adversely affecting its characteristics and
performance during its intended use".

This page is the entry point. It says what to do in order and links the
reference page that holds each detail. The
[information sheet](../evaluate/information-sheet.md) names the
manufacturer, the version, the intended purpose, the data categories and the
standards. Every quotation is from the Official Journal text, vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).

## Who reads which part

| You are | Read |
|---|---|
| The operator who installs and runs a deployment | every section below |
| The developer of a client application clinicians use | [Intended use](#intended-use), [Reading an answer safely](#reading-an-answer-safely), [Limitations](#limitations), and [The client contract](../integrate/client-contract.md) |
| A deployment's data protection officer or counsel | [Configuration for a deployment in the EU](#configuration-for-a-deployment-in-the-eu), [Limitations](#limitations), [Data protection](../evaluate/data-protection.md) and [Regulatory status](../evaluate/regulatory-status.md) |

## Intended use

FerroFED is used by healthcare providers, through the client applications
their health professionals use in patient care, to read and write a
patient's openEHR records across the clinical data repositories (CDRs) of a
federation. It answers an ordinary AQL query with one result set that names
the CDR each row came from, and routes follow-up reads and writes to the CDR
that holds the record
([Intended purpose](../evaluate/regulatory-status.md#intended-purpose)).

Use it only for that. FerroFED computes no score, flags no finding and
recommends nothing. A client application, a stored query an operator
publishes, or a change to FerroFED's code can bring a purpose of its own,
which is outside the manufacturer's intended purpose and is assessed on its
own ([The medical device question](../evaluate/data-protection.md#the-medical-device-question)).

## Before you install

1. **Check the platform.** FerroFED runs on Linux, `x86_64` or `aarch64`,
   as a container image or a static binary; nothing is built for Windows
   ([Supported platforms](deployment-shape.md#supported-platforms)). The
   gateway holds little in memory: about 2 MiB at rest in the measurement on
   [Memory footprint](container.md#memory-footprint). Each concurrent query
   holds its members' answers, up to `federation.max_node_answer_bytes` each
   ([The answer bound](overload.md#the-answer-bound)), so size memory from
   the sizing below.
2. **Gather the services it consumes:** two or more openEHR CDRs that serve
   ITS-REST 1.1.0, an identity provider for your callers, a PIX Manager, and
   an Audit Record Repository with the FHIR Feed of ITI-20
   ([What you need](production.md#1-what-you-need)).
3. **Agree the conditions of membership with each CDR's operator:** a unique
   `system_id`, version-4 UUID `ehr_id`s never reused or adopted, each
   `ehr_id` fed to the PIX Manager, and consent enforced at the CDR
   ([Prepare each CDR](production.md#prepare-each-cdr)).

## Installation

Follow [A production deployment](production.md) in order. Its steps are the
installation instructions:

1. Verify the image or binary against its attestation and pin it by digest
   ([Install](production.md#2-install)).
2. Write the registry with every endpoint suspended
   ([The registry](production.md#3-the-registry)).
3. Trust your callers' issuer
   ([Client authentication](production.md#4-client-authentication)).
4. Give each member its onward credential and make the signing key
   ([Onward credentials](production.md#5-onward-credentials)).
5. Configure identity resolution
   ([Identity resolution](production.md#6-identity-resolution)).
6. Send the access records to your Audit Record Repository
   ([The access log](production.md#7-the-access-log)).
7. Put TLS in front of the gateway, or on its listener
   ([TLS and the public address](production.md#8-tls-and-the-public-address)).
8. Run `config check`, start the gateway, and admit each member with
   `ferrofed admission check` before you take its endpoint out of suspension
   ([Check, start and admit](production.md#9-check-start-and-admit)).
9. Send a first query and read its `meta.federation`
   ([The first federated query](production.md#10-the-first-federated-query)).

`ferrofed config check` reads the configuration exactly as `serve` does and
exits `78`, naming the key, on anything it refuses. Run it before every
start and every upgrade. Work through the [hardening checklist](hardening.md)
before the gateway reaches real patient data.

## Configuration for a deployment in the EU

Four settings decide whether a deployment meets the EHDS Regulation where
the gateway can meet it. Add them to the configuration of the production
guide:

```toml
# ferrofed.toml
[federation.consent]
disclose = false               # Art 8: a restriction is never shown to a provider

[[auth.issuer]]                # the issuer of the production guide's step 4
issuer = "https://idp.example.org/realms/ferrofed"
jwks_uri = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs"

[auth.issuer.assurance]
claim = "acr"
minimum = "substantial"        # Annex II 3.1; "high" for cross-border from 26 March 2032
substantial = ["urn:example:loa:substantial"]
high = ["urn:example:loa:high"]

[audit]
destination = "repository"     # the access records of Annex II 3.2

[access_log.templates]
"Example Lab Report.v1" = ["medical-test-result"]
"Example Discharge.v1" = ["discharge-report"]

[access_log.retention]
years = 3                      # Art 9(2): at least three years from each access
```

- **`[federation.consent] disclose = false`.** Art 8 reads: "The fact that a
  natural person has restricted access ... shall not be visible to
  healthcare providers." The default, `true`, reports a member a consent
  pre-filter excludes as `consent-denied`, which the Federation Tier
  requires (N27a). In the EU set `false`
  ([Withholding consent exclusions](consent-exclusions.md)).
- **The assurance level, per issuer.** Annex II, point 3.1, asks for
  "reliable mechanisms for the identification and authentication of health
  professionals". Set `[auth.issuer.assurance]` for every issuer whose
  tokens reach patient data, with `minimum = "substantial"`; Implementing
  Regulation (EU) 2026/2099 Art 6(3) raises a cross-border exchange to
  `"high"` from 26 March 2032. List the values your issuer writes; no
  specification says which `acr` values stand for which level
  ([The assurance level](authentication.md#the-assurance-level)).
- **The access records to a repository.** Every access to patient data is
  recorded with the verified caller, the patient, the categories and the
  origins, and an access whose record cannot be stored is refused `503
  access-unrecorded` ([The access log](audit.md#the-access-log)).
- **A category map.** FerroFED ships none. Author one in `[access_log]`
  from the templates your members hold, or every access is recorded
  `ehds-unclassified` ([`[access_log]`](audit.md#access_log)).
- **The retention periods.** Art 9(2) keeps the information on each access
  "for at least three years from each date of access". Declare in
  `[access_log.retention]` any longer period your national law sets for a
  category or an origin, and keep each record at your repository until its
  `ehds-retention-ends` ([How long a record is kept](audit.md#how-long-a-record-is-kept)).
- **The receiving members.** Annex II, points 2.2 and 2.3, ask that an EHR
  system "be able to receive" data in the European exchange format. This
  release has no receive path, so there is no receiving member to configure;
  it is planned with a member declared per category
  ([#665](https://github.com/FerroHEALTH/FerroFED/issues/665)).

Keep two defaults, which the [claims review](../evaluate/claims-review.md#annex-ii-point-25)
assessed against Annex II, point 2.5:

- **Keep `[federation] best_effort` at its default, `true`.** With `false`,
  a client can no longer ask for a partial answer, so one silent member
  blocks every answer ([Completeness](queries-and-areas.md#completeness)).
- **List each member's consent refusal codes.** A member's consent refusal
  fails the whole query `424` until its codes are in the registry's
  `consent_refusal_codes` for that endpoint
  ([Consent](consent.md)). Ask each member's operator for them at admission.

## Sizing

The budgets and limits bound how long one request or one slow member can
hold the gateway. Set too low, they delay access; set too high, one slow
member holds the clinician's answer. Size them from what you measure:

| Setting | Default | How to size it |
|---|---|---|
| `federation.per_node_timeout_ms` | `10000` | above the slowest member's 99th percentile on `ferrofed_node_request_duration_seconds`, measured under your load ([Metrics](metrics.md#the-metrics)) |
| `federation.overall_timeout_ms` | `25000` | above `per_node_timeout_ms` plus the resolver's and the localizer's time on their duration histograms ([Timeouts](queries-and-areas.md#timeouts)) |
| `server.request_timeout_ms` | `30000` | more than one second above `overall_timeout_ms`; `config check` refuses less |
| `server.max_concurrent_requests` | `512` | the queries your members can take at once divided by the number of members each query asks ([The concurrency limit](overload.md#the-concurrency-limit)) |
| `federation.max_in_flight_per_node` | `64` | the requests the smallest member can serve at once; past it a member is reported `time-out` ([The per-member cap](overload.md#the-per-member-cap)) |
| `[server.caller_rate]` | off | set only where one caller can crowd out others ([The per-caller rate](overload.md#the-per-caller-rate)) |

After a change, watch `ferrofed_overload_refusals_total` and the
`FerroFEDOverloaded`, `FerroFEDMemberCapSaturated` and `FerroFEDSlowAnswers`
alerts ([Dashboard and alert rules](metrics.md#dashboard-and-alert-rules)).

## Reading an answer safely

These are the instructions a client application passes on to the
clinicians who use it. Each one is a control of the
[clinical safety risk file](../evaluate/clinical-safety.md).

- **Read `meta.federation.complete` before the rows.** `false` means a
  member that may hold the patient's data contributed nothing: it was not
  resolved, it was excluded on consent, or, under a partial answer, it did
  not answer. Show that the answer is incomplete, and name the members that
  did not contribute; `meta.federation.endpoints` lists each with its status
  ([What a client gets back](../integrate/client-contract.md#what-a-client-gets-back)).
- **Ask for a partial answer only where an incomplete record is safe to
  show.** By default a member that does not answer fails the query with no
  rows. `openEHR-federation-completeness: partial` returns the other
  members' rows with `complete: false` ([Completeness](queries-and-areas.md#completeness)).
- **Expect the same fact from two members.** Two CDRs can hold the same
  entry, for example a medication both recorded. The gateway returns both
  rows, each with its endpoint, unless the client asks for de-duplication
  by version identity, which removes only copies of one version. Show the
  origin of each row, and never add up quantities across members without
  checking for duplicates.
- **Show the origin.** The `openEHR-federation-endpoint` response header
  names the members that contributed rows, and a query that selects the
  `ENDPOINT` attributes gets each row's origin in the row
  ([Provenance columns](../integrate/client-contract.md#provenance-columns-endpoint-attributes)).
  A follow-up read or write goes to the CDR that holds the record.
- **Treat `not-resolved` as unknown.** In a deployment with
  `disclose = false`, a member the patient restricted reads exactly as a
  member that does not know the patient. Neither means the patient has no
  data there.
- **Read a `503 access-unrecorded` after a write as an unknown outcome.**
  The node may have stored the write before the gateway failed to record
  the access. Read the resource before writing it again
  ([Failing closed](audit.md#failing-closed)).

## Maintenance and its frequency

Art 30(1)(k) has the manufacturer inform users "of any mandatory preventive
maintenance of the EHR systems and its frequency". This release sets no
maintenance on a fixed calendar. The maintenance it needs is triggered by an
event, and its frequency is the event's:

| Maintenance | When | Reference |
|---|---|---|
| Upgrade to the newest release | at each release, and as soon as you can after a security advisory: security fixes go to the latest release only | [Upgrading](upgrading.md), [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md) |
| Run `config check` | before every start, reload and upgrade | [Running it](configuration.md#running-it) |
| Re-run `ferrofed admission check` for a member | when the member changes its CDR product, its version, its `system_id` or how it creates `ehr_id`s | [Admitting a node](admission.md) |
| Update the category map | when a member adds or changes a template | [`[access_log]`](audit.md#access_log) |
| Watch the alerts | continuously, on the shipped alert rules | [Dashboard and alert rules](metrics.md#dashboard-and-alert-rules) |
| Clear a quarantined audit record | when `ferrofed_audit_quarantined` rises, after reading why the repository refused it | [The audit spool](hardening.md#the-audit-spool) |
| Rotate the signing key | on your key policy's schedule, and at once when it may have leaked | [Rotating the signing key](onward-credentials.md#rotating-the-signing-key) |
| Renew TLS certificates and onward credentials | before each expires | [Hardening](hardening.md#tls) |
| Re-read this page and the information sheet | at each release | the [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md) |

When a release makes maintenance mandatory, the manufacturer says so in its
upgrade notes and through the channels of
[Complaints and incidents](../evaluate/post-market.md#corrective-action-recall-and-withdrawal).

## Limitations

Art 28(b) forbids "failing to inform the professional user of likely
limitations related to interoperability or security features of the EHR
system in relation to its intended purpose". The full list is
[Limitations](../evaluate/what-ferrofed-claims.md#limitations); the ones a
deployment in the EU must plan for are:

- **The European interoperability software component is not served.** The
  gateway produces and receives no document in the European exchange format
  (Annex II, points 2.1 to 2.3); the component is a library, and its open
  hazards are in the [clinical safety risk file](../evaluate/clinical-safety.md#the-interoperability-component).
- **The logging component is read at your Audit Record Repository.** The
  gateway has no review route of its own: the records are read there with
  ITI-81, and the repository sets who may read them and keeps each for the
  period it states (Annex II, points 3.3 and 3.4;
  [Reading the log](audit.md#reading-the-log)). A search by the person's
  identifier does not yet find a routed request addressed by `ehr_id`
  ([#796](https://github.com/FerroHEALTH/FerroFED/issues/796)), and the
  records carry no label for access rights by origin and category
  ([#797](https://github.com/FerroHEALTH/FerroFED/issues/797)).
- **The professional's identification and assurance level are not yet in
  the access record**
  ([#742](https://github.com/FerroHEALTH/FerroFED/issues/742)).
- **No EU declaration of conformity has been drawn up**, and no release
  carries the CE marking
  ([Regulatory status](../evaluate/regulatory-status.md#what-is-built-and-what-is-planned)).
- **The Federation Tier specification is a release candidate**, v0.9.0.

## Accessible formats

Recital 37 asks for instructions "in accessible formats for persons with
disabilities". These instructions are HTML text with headings, lists and
tables with header rows; no step is carried by an image or by colour
alone. The source is plain Markdown,
[`website/book/src/operate/instructions-for-use.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/website/book/src/operate/instructions-for-use.md),
and the book's print page renders every page as one document. Ask the
[single point of contact](../evaluate/post-market.md#the-manufacturer) for
another format.

## Problems and complaints

When the gateway refuses to start, answers with an error code, or logs a
failure, look the symptom up on [Troubleshooting](troubleshooting.md): it
leads from the status, the code or the log line to the cause and the
setting that fixes it.

Report a problem, a complaint or a possible serious incident through the
channels on [Complaints and incidents](../evaluate/post-market.md#making-a-complaint).
Never send patient data.
