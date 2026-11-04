<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Laboratory reports over FHIR

Medical test results are a priority category of Regulation (EU) 2025/327
(Art 14(1)(e)). The [document list](patient-summary.md#the-document-list)
names a patient's laboratory reports beside the patient summary, each at a
`Binary/{token}` URL the gateway signs for you
([Reading a member's document](patient-summary.md#reading-a-members-document)).

## Laboratory reports in the list

When a `[[fhir.receive]]` entry receives `Laboratory-Reports`
([A laboratory report](../evaluate/receiving-documents.md#a-laboratory-report)),
or a member serves the category at its own endpoint
([A member's own laboratory reports](#a-members-own-laboratory-reports)),
the list also names each member's laboratory reports. The gateway compiles
the laboratory mapping `eehrxf` ships against that entry's template at
start, and `config check` refuses a template it does not compile against.
A `type` of `11502-2`, `http://loinc.org|11502-2` or `http://loinc.org|`
admits them, and a `type` that admits only them lists no summary.

The gateway runs the reserved stored query
`eu.ferrofed.eehrxf::laboratory-report` on every member the patient
resolves at, by each member's own `ehr_id` alone, and lists one entry per
laboratory composition: typed LOINC `11502-2` in the `Laboratory-Reports`
category, its custodian the member's organisation, its `masterIdentifier`
`urn:uuid:` and the UUID of the composition's versioned object, its
`format` the `Bundle-eu-lab` profile, and its `attachment.url` a
`Binary/{token}` the gateway signs for you, naming the member, its `ehr_id`
and the composition. A composition two members hold under one version id
is listed once, from the member whose `system_id` created it. A member
that does not answer the query fails the list under all-or-nothing, `424`
or `504` naming it, and is named in an `outcome` entry under `partial`
(`fhir/receive/laboratory.rs`:
`a_members_laboratory_composition_is_listed_and_read_as_a_laboratory_report`,
`a_member_silent_at_the_laboratory_query_fails_the_list_and_is_named`).

Reading that URL asks the member alone for the composition
(`GET {base}/ehr/{ehr_id}/composition/{uid}`) and writes it as an HL7
Europe Laboratory Report 2.0.0 `Bundle-eu-lab` document: the gateway's
`Device` and the operator author it, its `DiagnosticReport-eu-lab` and
results are the mapping's, its results are listed in a `Laboratory
studies` section, one `Provenance` names the member, and its `Patient` is
the identifier the identity binding names behind the `ehr_id` in
`fhir.receive_namespaces` (`424` when it names none). The report is
written on each read and kept nowhere. Each list and read is recorded
under `Laboratory-Reports` in the access log.

The component work behind these reports is FerroBRIDGE's, through the
published `eehrxf` crate: the
[laboratory report crosswalk](https://ferrobridge.eu/docs/evaluate/category-crosswalks.html#the-laboratory-report-crosswalk)
to the HL7 Europe Laboratory Report profiles, the
[mapping out of openEHR](https://ferrobridge.eu/docs/evaluate/laboratory-mapping.html#mapping-out-of-openehr)
the read writes the report through, and
[receiving a laboratory report](https://ferrobridge.eu/docs/evaluate/laboratory-mapping.html#receiving-a-laboratory-report)
into openEHR, which the gateway's
[receipt](../evaluate/receiving-documents.md#a-laboratory-report) uses.

## A member's own laboratory reports

A member can serve its laboratory reports itself, as an HL7 Europe EU
Health Data API Document Access Provider. Declare an `hl7-fhir-rest`
endpoint of the member with `categories = ["Laboratory-Reports"]`, the
`Bundle-eu-lab` profile and the identifier system of its `ehr_id`, as for a
[member that serves its own summary](patient-summary.md#a-member-that-serves-its-own-summary).
The gateway then asks that member at its own endpoint and leaves it out of
the laboratory report query, so no report is listed twice. It lists the
member's reports with or without a `[[fhir.receive]]` entry for the
category.

The gateway asks the member by its own `ehr_id` alone, with the
interactions the Document Access Provider capability statement makes
SHALL:

1. `GET {endpoint}/Patient?identifier={system}|{ehr_id}` for the logical id
   of the member's `Patient`. An empty `searchset` means the member holds no
   report of the patient, and nothing more is asked.
2. `GET {endpoint}/DocumentReference?patient={id}&type=http://loinc.org|11502-2`
   for its laboratory reports, each `next` page followed under its own base.

The list names each `DocumentReference` the member answers as the member
holds it, with one change: each `content.attachment.url` becomes a
`Binary/{token}` the gateway signs for you, naming the member, its
`ehr_id` and the logical id of the member's `Binary`. The member's own URL
is one you cannot reach, and a direct link would pass the gateway's access
check and access record by. Every other element keeps its value, numbers
keep the text the member wrote, and JSON member order is not kept, since
RFC 8259 gives it no meaning. Reading that URL asks the member alone for
`GET {endpoint}/Binary/{id}` and answers its bytes and media type as the
member sent them.

An attachment URL that names no `Binary` under the member's endpoint, or a
`Patient` search that names more than one patient, makes the member one in
`node-error`. A member that does not answer fails the list under
all-or-nothing, `424` or `504` naming it, and is named in an `outcome`
entry under `partial`. Each list and read is recorded under
`Laboratory-Reports` in the access log, naming the member's own endpoint
and its `ehr_id` (`fhir/list/own_laboratory.rs`).
