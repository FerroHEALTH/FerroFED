<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED claims

FerroFED claims what a release ships and a test holds. This page lists the
standing decisions, what each release shipped, and what is planned, so you
can tell them apart. The
[changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
has the detail of every change.

## Decided

The architecture of record was decided on 2026-10-01 and is
[`docs/architecture.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/architecture.md).
These decisions hold across every release:

- **The specification is the authority.** The Federation Tier with AQL text
  decides; its reference implementation is read as evidence, never as an
  oracle. Where the specification is silent, the decision is FerroFED's own
  and is labelled that way in the code and in this book.
- **The openEHR surface comes from the published `openehr-*` crates.** The
  ITS-REST contract, the AQL parser and printer, the RM and the typed
  identifiers are those crates. A gap in one of them is fixed in that crate.
- **No clinical data of its own.** The gateway holds the registry, the
  `ehr_id` index and the stored-query definitions it is authoritative for.
  The record stays on the nodes.
- **Identifier hygiene is a hard rule.** Nothing the gateway composes for a
  node carries a directly identifying patient identifier, and every carrier
  the specification names has a negative test (§5.4, N33).
- **Pure Rust, a single binary**, as across the FerroHEALTH family.

## Shipped

### v0.0.3: the federated query and identity resolution

- `POST {base}/v1/query/aql` answers one ITS-REST `RESULT_SET` over every
  member of a registry, with no federation syntax needed (N1, CP-1).
- The patient is resolved outside AQL through an IHE PIXm PIX Manager
  (ITI-83) on either patient-identifier carrier, and each member that knows
  the patient receives standard AQL keyed on its own `ehr_id` (§5, §7.1, N7).
- The rewrite refuses a query that would carry the identifier to a node, and
  an outbound gate checks every request again before it leaves (§5.4, CP-26).

### v0.0.6: the federated answer, routing and targeting

v0.0.6 carries the milestones v0.0.4, v0.0.5 and v0.0.6; no v0.0.4 or v0.0.5
was tagged.

- The full per-endpoint report in `meta.federation`, all-or-nothing
  completeness with the best-effort opt-in, the per-node and overall budgets
  with `Prefer: wait`, and the §11.2 statuses with stable error codes (§9.5,
  §11.1 to §11.5).
- `ORDER BY` with `LIMIT` re-applied at the Tier, bounded `OFFSET` pages,
  `SELECT DISTINCT`, recombined `COUNT`, `SUM`, `MIN`, `MAX` and `AVG`, and
  opt-in version-identity de-duplication (§10, §11.6).
- Routing of the EHR resources under a path `ehr_id` to the one node that
  holds the EHR, byte for byte, through the targeting headers, the `ehr_id`
  index and the ask-all probe; the `creating_system_id` map learned from
  every answer (§7a.1, §12.2, §12.5).
- The `FROM ENDPOINT` and `ORGANISATION` directive, the targeting headers,
  and ENDPOINT attributes in rows (§8, §9.3).
- The registry document in FHIR form, `OPTIONS {base}/`, and the stored-query
  registry with immutable versions invoked by name (N19, §7a.2, §12.7).
- Track 10, the identifier-leakage suite, run against two FerroEHR nodes.

### v0.0.7: definitions and membership

Released on 2026-10-03.

- Definition requests routed to one named node, the opt-in fan-out template
  upload, and stored-query distribution with drift reporting and an
  operator's repair (§12.6, §12.7, N43, N44).
- Versioned writes, `CONTRIBUTION`s included, sent only to the CDR that
  controls the version; a new EHR created only at a named node; `ehr_id`
  collisions refused and raised as integrity incidents (§12.4, §12.5.2,
  §12a.1, N23, N42).
- `GET {base}/v1/ehr` by subject, the `GET` forms of query execution, both
  `ehr_id` forms of N29, the provenance headers, a configurable base path, and
  the DEMOGRAPHIC area routed to one declared endpoint (§4.1, §7a.3, N28,
  N29, N31, N32).
- `ferrofed admission check` for the identifier-integrity conditions of
  §12b.2 (N42a), the registry reload on `SIGHUP`, the stored-query registry
  on `redb`, PostgreSQL or read-only files, health probes, and metrics.

### v0.0.8, on `main`, in the next release

The v0.0.8 milestone, security and the bindings (§13 to §15, Annex A,
Annex B), is in progress. `main` carries these parts of it, and v0.0.8 is
cut from `main` once the milestone closes.

- Client authentication at the gateway: RFC 9068 access tokens checked by
  key set or by introspection, SMART on openEHR scopes per route, the purpose
  of use, and an explicit edge mode for a proxy that authenticates callers
  (§13.1, N25, CP-17;
  [#80](https://github.com/FerroHEALTH/FerroFED/issues/80),
  [Client authentication](../operate/authentication.md)).
- OAuth 2.0 client credentials to each node with an RFC 7523 assertion
  signed ES384, the node tokens cached, and the gateway's JWK Set published
  at `{base}/.well-known/jwks.json` (§13.1, N25, N30, CP-17;
  [#81](https://github.com/FerroHEALTH/FerroFED/issues/81)).
- RFC 8693 token exchange per endpoint, which gives each verified caller a
  token of its own at a node, and node tokens bound to a key of the
  gateway's with DPoP (RFC 9449), the proof written and a node's nonce
  answered by the `openehr-its` 0.0.83 client
  ([#439](https://github.com/FerroHEALTH/FerroFED/issues/439),
  [#448](https://github.com/FerroHEALTH/FerroFED/issues/448),
  [Onward credentials](../operate/onward-credentials.md)).
- The two authentication tracks of the Dutch binding as onward credentials:
  the Nuts grant, a Verifiable Presentation of the gateway's credentials for
  a DPoP-bound token (Annex B §B.4, Nuts RFC021), and the FAPI 2.0 grant of
  the BgZ/eOverdracht track: client credentials or token exchange with a
  `private_key_jwt` assertion, DPoP-bound tokens, and the healthcare
  attributes in an RFC 9396 `authorization_details` object (Annex B §B.4a)
  ([#88](https://github.com/FerroHEALTH/FerroFED/issues/88),
  [#497](https://github.com/FerroHEALTH/FerroFED/issues/497),
  [The Nuts grant](../operate/onward-credentials.md#the-nuts-grant-annex-b-b4),
  [The FAPI 2.0 grant](../operate/onward-credentials.md#the-fapi-20-grant-annex-b-b4a)).
- A `patient/` grant honoured only for an issuer the deployment binds to one
  member: the token's `ehrId` resolved through the cross-reference at every
  member (§5.2), every request held to the patient's own `{node, ehr_id}`
  pairs with nothing sent beyond them, and each node told the patient's
  `ehr_id` there; a bare `ehrId` is never compared across members (§12.5,
  N26;
  [#443](https://github.com/FerroHEALTH/FerroFED/issues/443),
  [Patient grants](../operate/authentication.md#patient-grants)).
- The caller's identity conveyed on every request to a node, in a token the
  gateway signs for that node and a node verifies against the published JWK
  Set (§13.1, N24, CP-16;
  [#82](https://github.com/FerroHEALTH/FerroFED/issues/82),
  [What a node is told about the caller](../operate/authentication.md#what-a-node-is-told-about-the-caller)).
- Consent left to the node, with the optional Step-1 pre-filter on every
  patient route, the read of an EHR by subject included, its outage carried
  in `meta.federation`, on the health report and in the metrics, and a
  node's own refusal reported as `consent-denied` (§13.2, N27, N27a;
  [#83](https://github.com/FerroHEALTH/FerroFED/issues/83),
  [#399](https://github.com/FerroHEALTH/FerroFED/issues/399),
  [#400](https://github.com/FerroHEALTH/FerroFED/issues/400)).
- Mitz as that pre-filter in the Netherlands: the closed authorization
  question asked once per data holder, a member Mitz denies reported
  `consent-denied` and never asked (Annex B §B.6, N27a;
  [#475](https://github.com/FerroHEALTH/FerroFED/issues/475),
  [Dutch consent](../operate/identity.md#dutch-consent-nl_gfmitz)).
- The §13.4 deployment decisions, answered for the gateway, with an
  operator's template for the rest (CP-39;
  [#84](https://github.com/FerroHEALTH/FerroFED/issues/84),
  [The §13.4 deployment decisions](../operate/deployment-decisions.md)).
- Undirected patient queries localized by IHE XCPD ITI-55 or by the PIXm
  resolver, which shares one ITI-83 answer between localization and
  resolution. A member no localizer names is `not-localized` and never
  asked, and a localizer that does not answer fails closed, with its error on
  every member and in `meta.federation`. The read of an EHR by subject is
  localized too, the localizer is on the health report and in the metrics,
  and every XCPD exchange is audited to a log target (N4, N10, §14.1, §14.2,
  Annex A.3, CP-5;
  [#85](https://github.com/FerroHEALTH/FerroFED/issues/85),
  [#408](https://github.com/FerroHEALTH/FerroFED/issues/408),
  [#409](https://github.com/FerroHEALTH/FerroFED/issues/409),
  [#410](https://github.com/FerroHEALTH/FerroFED/issues/410)).
- NVI localization in the Netherlands: an undirected patient query asks the
  national index which care providers hold data for the patient's
  pseudonym, and the members that hold those providers' data are the
  candidates, with the custodian map read from the URAs a care services
  directory with the LRZa as its source publishes (Annex B §B.1, §B.2;
  [#87](https://github.com/FerroHEALTH/FerroFED/issues/87),
  [Dutch localization](../operate/identity.md#dutch-localization-nl_gfnvi)).
- A PDQm step ahead of localization and resolution, for a patient named by
  an identifier the cross-reference does not know: the Patient Demographics
  Supplier is asked with ITI-78 or ITI-119 for the patient's identifier in
  the master domain, and an ambiguous match is never picked (Annex A §A.2,
  §A.7;
  [#487](https://github.com/FerroHEALTH/FerroFED/issues/487),
  [Demographics first](../operate/identity.md#demographics-first-pdqm)).
- The XCPD audit sent to an ATNA Audit Record Repository with ITI-20: the
  DICOM PS3.15 audit message in RFC 5424 syslog over TLS, stored in a
  bounded spool on disk first and delivered from it, so a repository outage
  fails no discovery and a full spool fails it closed (ITI TF-2 §3.20,
  §3.55.5.1.1;
  [#418](https://github.com/FerroHEALTH/FerroFED/issues/418),
  [#512](https://github.com/FerroHEALTH/FerroFED/issues/512),
  [The audit repository](../operate/identity.md#the-audit-repository)).
- Every other IHE transaction the gateway makes or receives audited as its
  profile requires: ITI-83, ITI-78, ITI-119, ITI-90, ITI-91, ITI-93 and
  ITI-94, each as a FHIR `AuditEvent` on the IHE Basic Audit Log Patterns
  (BALP), sent to the repository with the FHIR Feed of ITI-20 through the
  same spool, and a transaction whose record cannot be stored failed
  ([#486](https://github.com/FerroHEALTH/FerroFED/issues/486),
  [#469](https://github.com/FerroHEALTH/FerroFED/issues/469),
  [The audit trail](../operate/audit.md)).
- The registry read from an IHE mCSD care services directory with ITI-90 and
  kept in step with ITI-91, a shared directory included, with in-scope
  deletions recorded in the replica so a dangling listing is judged the same
  at a start and on a refresh, and a deleted endpoint dropped (§15.1, §15.2,
  N21, Annex A.5;
  [#86](https://github.com/FerroHEALTH/FerroFED/issues/86),
  [#423](https://github.com/FerroHEALTH/FerroFED/issues/423),
  [#433](https://github.com/FerroHEALTH/FerroFED/issues/433)).
- The §12.5.1 resolution bindings kept per verified caller (N41;
  [#412](https://github.com/FerroHEALTH/FerroFED/issues/412)).
- The identity lifecycle over IHE PMIR: an ITI-94 subscription at a Patient
  Identity Registry, and an authenticated ITI-93 feed whose merges and
  deletes drop the resolution bindings they could have made stale; track 8
  stays deferred, since the specification marks it provisional (Annex A.4;
  [#147](https://github.com/FerroHEALTH/FerroFED/issues/147),
  [The identity feed](../operate/identity.md#the-identity-feed-pmir)).
- A credential or a patient identifier sent only over `https` outside the
  development profile, and the stored-query store's PostgreSQL password only
  over TLS
  ([#402](https://github.com/FerroHEALTH/FerroFED/issues/402),
  [#416](https://github.com/FerroHEALTH/FerroFED/issues/416)).
- A Kubernetes example that starts, checked in CI
  ([#428](https://github.com/FerroHEALTH/FerroFED/issues/428)).
- Traces exported through OpenTelemetry over OTLP, with a sample ratio and a
  `traceparent` of the gateway's own trace on every node request; nothing
  of a client's trace context is recorded, so no client-chosen trace id
  reaches the collector
  ([#353](https://github.com/FerroHEALTH/FerroFED/issues/353),
  [#437](https://github.com/FerroHEALTH/FerroFED/issues/437),
  [#446](https://github.com/FerroHEALTH/FerroFED/issues/446),
  [Tracing](../operate/tracing.md)).
- The texts this work cites, vendored with their provenance: the SMART on
  openEHR source and the IHE IUA supplement
  ([#414](https://github.com/FerroHEALTH/FerroFED/issues/414)), PMIR, BALP,
  the Dutch Generic Functions IG, the Nuts specifications, and the country
  research corpora
  ([#498](https://github.com/FerroHEALTH/FerroFED/pull/498)). The Mitz
  documents state no licence, so they are pinned by digest and fetched into
  a local cache, never committed
  ([#501](https://github.com/FerroHEALTH/FerroFED/issues/501)).

## Planned

Each milestone on the
[milestones page](https://github.com/FerroHEALTH/FerroFED/milestones) is a
release, and every issue in it names the sections it answers.

The rest of v0.0.8: one TLS type and one credential mapping for every
outbound client, with mutual TLS toward the PIX Manager, the PMIR Registry
and the mCSD directory
([#507](https://github.com/FerroHEALTH/FerroFED/issues/507)), and an audit
record still queued behind another spool write when its exchange's time
runs out, which is dropped silently today
([#532](https://github.com/FerroHEALTH/FerroFED/issues/532)).

A `CONTRIBUTION` in canonical XML waits on the openEHR XSD and `openehr-its`
([#308](https://github.com/FerroHEALTH/FerroFED/issues/308)).

v0.0.9, conformance (§16, §17): every conformance point scored
([#89](https://github.com/FerroHEALTH/FerroFED/issues/89)), the Connectathon
tracks as runnable suites
([#91](https://github.com/FerroHEALTH/FerroFED/issues/91),
[#92](https://github.com/FerroHEALTH/FerroFED/issues/92)), the node profile
([#93](https://github.com/FerroHEALTH/FerroFED/issues/93)), a differential run
against the reference implementation
([#94](https://github.com/FerroHEALTH/FerroFED/issues/94)), the conformance
statement ([#95](https://github.com/FerroHEALTH/FerroFED/issues/95)), and an
operator and query console
([#274](https://github.com/FerroHEALTH/FerroFED/issues/274)).

## Not claimed

FerroFED claims a conformance point only when a test carries its marker and CI
runs it. The [conformance matrix](conformance.md) records where each point
stands, and the [obligations checklist](obligations.md) does the same for
every normative statement of the specification. The specification is a
release candidate; when 1.0 is published, the vendored text is re-pinned and
every citation is checked against it
([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)).
