<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED claims

**Audience:** decision makers, compliance and procurement readers, and
operators before a deployment. FerroFED claims what a release ships and a
test holds. This page lists the standing decisions, the limitations to know
before you deploy it, and what it does not claim. The
[changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
says what each release shipped.

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


## Limitations

Regulation (EU) 2025/327 Art 28(b) forbids "failing to inform the
professional user of likely limitations related to interoperability or
security features of the EHR system in relation to its intended purpose".
These are the ones to know before you deploy FerroFED; the
[claims review](claims-review.md) records how this list is kept.

- **The specification is release 1.0.1.** FerroFED implements the 1.0.1
  release of the Federation Tier with AQL, the erratum of 2026-11-01 to
  the 1.0.0 release the openEHR SEC approved on 2026-10-12, and reports
  `spec_version` `"1.0"` in `OPTIONS {base}/`, since a patch release keeps
  the wire contract (§7a.2). Upstream has not tagged 1.0.1, so the
  vendored text is its untagged release commit
  ([#1478](https://github.com/FerroHEALTH/FerroFED/issues/1478)).
  Releases up to v0.0.15 implement 1.0.0, and releases up to v0.0.9 the
  0.9.0 release candidate, which report `"0.9"`; v0.0.10 re-pinned it
  ([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)).
- **No EHDS harmonised component is complete.** The gateway records every
  access to patient data as Annex II, point 3.2, asks of the European
  logging software component (Art 25(1)), read at your Audit Record
  Repository with ITI-81 (point 3.3), each record stating its retention by
  origin and category (point 3.4). A query over many patients' data names
  the patients its delivered rows project by `ehr_id`, up to a bound, and
  otherwise records only its scope, which shows a person that a query ran
  over a member holding their EHR, never that it read their record.
  Each record carries a security label per category and per
  origin, which your repository applies as access rights
  ([Labelling the access records](../operate/access-labels.md)). The
  European interoperability software component writes the patient
  summary and receives a document in the exchange format on the gateway's
  FHIR face, and passes prescriptions and dispensations through from the
  members that serve them in the HL7 Europe MPD profiles on their own FHIR
  endpoint, or maps them from a member's openEHR medication data in the
  map mode
  ([Prescriptions and dispensations over FHIR](../integrate/prescriptions.md)).
  It lists, reads and receives laboratory reports
  ([Laboratory reports over FHIR](../integrate/laboratory-reports.md)),
  with hazard I-16 of the [clinical safety risk file](clinical-safety.md)
  open and I-18 closed at S4, P1. It lists and reads discharge and
  imaging reports and receives both
  ([Discharge reports over FHIR](../integrate/discharge-reports.md),
  [Imaging reports over FHIR](../integrate/imaging-reports.md)), with
  hazards I-19 and I-20 closed at S4, P1 ([#1826](https://github.com/FerroHEALTH/FerroFED/issues/1826)) and I-21 and
  I-22 closed at S4, P1 ([#2065](https://github.com/FerroHEALTH/FerroFED/issues/2065)). No EU
  declaration of conformity has been drawn up, and no release carries the
  CE marking
  ([Regulatory status](regulatory-status.md#what-is-built-and-what-is-planned)).
- **The access records are kept by your Audit Record Repository.** Every
  federated query, stored-query execution, routed read and routed write
  that reaches a node, a query from the operator console included, is
  recorded with the verified caller and the patient, and an access whose
  record cannot be stored is refused `503 access-unrecorded`, a write
  before any member is asked
  ([The access log](../operate/access-log.md)). The gateway keeps
  no copy, so who reads the records is the repository's, and it keeps each
  for the period the record states from `[access_log.retention]`, at least
  three years ([How long a record is kept](../operate/access-log.md#how-long-a-record-is-kept)).
  FerroFED ships no category map: until you declare one in
  `[access_log]`, the categories of the data an access reached are recorded
  `ehds-unclassified`. The record names the professional the token names,
  and the assurance level when the issuer declares one
  ([The person behind an access](../operate/access-log.md#the-person-behind-an-access)).
  An access whose token declares a purpose of use you name in
  `[[access_log.emergency_purpose]]` is marked an emergency access (Art
  11(5)); FerroFED names none, and the mark records what the caller
  asserted, never whether restricted data were released, which the node
  decides ([Emergency access](../operate/access-log.md#emergency-access)).
- **The assurance level is yours to set.** Patient data needs a natural
  person behind the token, or a client application you declare to act for
  the professional its token names; any other token is refused
  `401 natural-person-required`. The gateway reads and requires an
  assurance level only for an issuer with `[auth.issuer.<name>.assurance]`, and
  `config check` names each issuer without one
  ([Professionals and assurance](../operate/authentication.md#professionals-and-assurance)).
  Your issuer authenticates the person; the gateway verifies the token.
- **Consent exclusions are shown by default.** A member a consent pre-filter
  excludes is reported `consent-denied`, as the specification requires
  (N27a). Art 8 of the Regulation forbids showing that restriction to a
  healthcare provider, so a deployment in the EU sets
  `[federation.consent] disclose = false`
  ([Withholding consent exclusions](../operate/consent-exclusions.md)). A
  node's own consent refusal fails the query `424` until you list the codes
  that node uses ([Consent](../operate/consent.md)).
- **One silent member fails the query.** A federated query is all-or-nothing
  by default (N37): when a member that was asked does not answer, the client
  gets no rows. A client that accepts a partial answer asks for one per
  request ([Queries and API areas](../operate/queries-and-areas.md#completeness)).
- **A write body travels as the client sent it.** The gateway keeps the
  patient identifier out of every request it composes for a node, but a
  committed `COMPOSITION` reaches its node byte for byte, a `DV_IDENTIFIER`
  inside it included (§5.4, N22;
  [Where the patient identifier stops](../how-it-works/identifier-hygiene.md#what-the-outbound-gate-reads)).
- **A received document is checked only as far as the validator can
  evaluate it.** A constraint `ferrobridge-validator` cannot evaluate is
  listed in the outcome of the write as a `warning`, not refused, and a
  `required` binding the packages cannot decide is listed as not evaluated
  when no terminology server is configured
  ([The check against the profiles](receiving-documents.md#what-the-component-does)).
- **Part of ITS-REST is not served.** The gateway never federates the
  DEMOGRAPHIC area: it answers `501` there unless you name one member to
  serve it, and `501` for any ITS-REST path it does not serve
  ([The DEMOGRAPHIC area](../operate/queries-and-areas.md#the-demographic-area)).
- **The listener speaks plain HTTP by default.** Terminate TLS in front of
  the gateway, or set `[server.tls]`, with a client CA to admit only your
  proxy, when the hop from the proxy to the gateway crosses a network
  others share
  ([TLS on the listeners](../operate/listener-tls.md#tls-on-the-listeners)).
- **Members in several Member States are not detected.** FerroFED neither
  detects nor refuses a federation whose members sit in different Member
  States ([Regulatory status](regulatory-status.md#cross-border-access)).

## Not claimed

FerroFED claims a conformance point only when a test carries its marker and CI
runs it. The [conformance statement](conformance-statement.md) is that claim
for the Federation-Gateway profile, with every deferral and its reason. The
[conformance matrix](conformance.md) records where each point
stands, and the [obligations checklist](obligations.md) does the same for
every normative statement of the specification. Both are held to the 1.0.1
release, to which the vendored text was re-pinned from 1.0.0
([#1478](https://github.com/FerroHEALTH/FerroFED/issues/1478)), itself
re-pinned from the 0.9.0 release candidate
([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)).
