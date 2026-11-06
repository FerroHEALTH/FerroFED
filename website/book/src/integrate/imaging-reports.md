<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Imaging reports over FHIR

Medical imaging studies and related reports are a priority category of
Regulation (EU) 2025/327 (Art 14(1)(d)). The
[document list](patient-summary.md#the-document-list) names the imaging
documents of a member that serves them itself, beside the patient summary,
each at a `Binary/{token}` URL the gateway signs for you
([Reading a member's document](patient-summary.md#reading-a-members-document)).

The gateway does not yet write an imaging report from a member's openEHR
compositions. That waits on the component's mapping out of openEHR, which
FerroBRIDGE builds in the published `eehrxf` crate, so a member that holds
its imaging reports only as openEHR compositions lists none.

## Imaging documents in the list

The HL7 Europe EU Health Data API 1.0.0-ballot types an imaging document by
the LOINC codes of `eehrxf-document-type-medical-imaging-vs`: `85430-7`
(diagnostic imaging report) and `18748-4` (diagnostic imaging study). A
`type` of either code, alone or in the LOINC system, or
`http://loinc.org|`, admits them, and a `type` that admits only them lists
no summary. With no `type`, the list names every document type it serves.

## A member's own imaging documents

A member can serve its imaging documents itself, as an HL7 Europe EU Health
Data API Document Access Provider. Declare an `hl7-fhir-rest` endpoint of
the member with `categories = ["Medical-Imaging"]`, the
`BundleReportEuImaging` profile
(`http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging`)
and the identifier system of its `ehr_id`, as for a
[member that serves its own summary](patient-summary.md#a-member-that-serves-its-own-summary).

The gateway asks the member by its own `ehr_id` alone, with the
interactions the Document Access Provider capability statement makes
SHALL:

1. `GET {endpoint}/Patient?identifier={system}|{ehr_id}` for the logical id
   of the member's `Patient`. An empty `searchset` means the member holds no
   imaging document of the patient, and nothing more is asked.
2. `GET {endpoint}/DocumentReference?patient={id}&type=http://loinc.org|85430-7,http://loinc.org|18748-4`
   for its imaging documents, each `next` page followed under its own base.
   The `type` names the codes of the value set your search admits, ORed
   with a comma as FHIR R4 search allows of every parameter.

The list names each `DocumentReference` the member answers as the member
holds it, with one change: each `content.attachment.url` becomes a
`Binary/{token}` the gateway signs for you, naming the member, its `ehr_id`,
the `Medical-Imaging` category and the logical id of the member's `Binary`.
Reading that URL asks the member alone for `GET {endpoint}/Binary/{id}` and
answers its bytes and media type as the member sent them. The rules are
those of
[a member's own laboratory reports](laboratory-reports.md#a-members-own-laboratory-reports):
an attachment URL that names no `Binary` under the member's endpoint, or a
`Patient` search that names more than one patient, makes the member one in
`node-error`, and a member that does not answer fails the list under
all-or-nothing, `424` or `504` naming it, and is named in an `outcome`
entry under `partial`. Each list and read is recorded under
`Medical-Imaging` in the access log, naming the member's own endpoint and
its `ehr_id` (`fhir/list/own_imaging.rs`).

## The component behind the category

The component work for imaging is FerroBRIDGE's, through the published
`eehrxf` crate: the
[imaging crosswalks](https://ferrobridge.eu/docs/evaluate/category-crosswalks.html#the-imaging-crosswalks)
to the HL7 Europe Imaging Report profiles, and
[what the component accepts](https://ferrobridge.eu/docs/evaluate/receiving-documents.html#what-the-component-accepts)
when it receives a document.
