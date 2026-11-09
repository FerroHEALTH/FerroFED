<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Discharge reports over FHIR

Discharge reports are a priority category of Regulation (EU) 2025/327
(Art 14(1)(f)). The [document list](patient-summary.md#the-document-list)
names the discharge reports of a member that serves them itself, beside
the patient summary, each at a `Binary/{token}` URL the gateway signs for
you ([Reading a member's document](patient-summary.md#reading-a-members-document)).

The gateway also writes a discharge report from a member's openEHR
composition when you declare the template those compositions are of
([Discharge reports the gateway writes](#discharge-reports-the-gateway-writes)).
It also receives a discharge report and writes it to the member you
declare for the category
([Discharge reports the gateway receives](#discharge-reports-the-gateway-receives)).

## Discharge reports in the list

The HL7 Europe EU Health Data API 1.0.0-ballot types a discharge report by
the LOINC codes of `eehrxf-document-type-discharge-report-vs`: `18842-5`
(discharge summary) and `100719-4` (surgical oncology discharge summary).
A `type` of either code, alone or in the LOINC system, or
`http://loinc.org|`, admits them, and a `type` that admits only them lists
no summary. With no `type`, the list names every document type it serves.

## Discharge reports the gateway writes

Declare the template of the members' discharge compositions in a
`[[fhir.report]]` entry:

```toml
[[fhir.report]]
category = "Discharge-Reports"
template = "/etc/ferrofed/templates/ferrobridge_discharge_report.adlt"
archetypes = [
  "/etc/ferrofed/archetypes/openEHR-EHR-COMPOSITION.transfer_summary.v1.adl",
  "/etc/ferrofed/archetypes/openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl",
  "/etc/ferrofed/archetypes/openEHR-EHR-ACTION.procedure.v1.adl",
  # and every archetype the template specialises or fills a slot with
]
```

- `template` is an OPT 1.4 `.opt`, or an ADL 2 template beside the
  `archetypes` it specialises or fills a slot with. The shipped HDR mapping
  of `eehrxf` maps FerroBRIDGE's discharge report template,
  `openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.0.0`, a transfer
  of care summary whose context is the hospital stay, with a diagnostic
  summary, significant procedures and discharge medications section.
- The configuration refuses an entry of a category the gateway writes no
  report of, a category named twice, a file it cannot read, and a template
  the mapping does not compile against. A `Laboratory-Reports` entry takes
  the place of the template of the category's `[[fhir.receive]]` entry,
  and a `Medical-Imaging` entry declares the
  [imaging reports](imaging-reports.md#imaging-reports-the-gateway-writes)
  the gateway writes.
- `fhir.receive_namespaces` names the identifier systems the report names
  its patient in.

The list then runs the reserved stored query
`eu.ferrofed.eehrxf::discharge-report` on every member the patient resolves
at that does not serve its discharge reports itself. It selects every whole
composition of `openEHR-EHR-COMPOSITION.transfer_summary.v1`, by the
member's own `ehr_id` alone. The list names one `DocumentReference` per
composition, typed `18842-5`, in the category `Discharge-Reports`, its
format `bundle-eu-hdr` and its master identifier the composition's
versioned object as a `urn:uuid:`. A search whose `type` admits only
`100719-4` asks no member this query. A composition two members hold under
one version id is listed once, from the member that created it.

Reading the URL asks that member alone for the composition and writes it
as an HL7 Europe Hospital Discharge Report 1.0.0 `bundle-eu-hdr` in the
composition's language: a `composition-eu-hdr` authored by the gateway's
`Device` and the operator, about a `Patient` named by the identifier the
identity binding holds behind the member's `ehr_id`, with the stay as its
`encounter-eu-hdr` and each diagnosis, procedure and discharge medicine in
its section. A hospital course section says the member's record holds none
the gateway writes, since `composition-eu-hdr` requires a discharge
summary or a hospital course. One `Provenance` names the member for every
mapped entry. A composition that does not map answers `424`, and a member
whose binding names no patient in `fhir.receive_namespaces` answers `424`.
Completeness and the access record follow the laboratory report's
([Laboratory reports the gateway writes](laboratory-reports.md)).

## Discharge reports the gateway receives

Declare the member that stores received discharge reports in a
`[[fhir.receive]]` entry, with the HL7 Europe Hospital Discharge Report
1.0.0 package, its dependency closure, FerroBRIDGE's ADL 2 discharge report
template with its archetypes, and the HDR project `eehrxf` ships:

```toml
[[fhir.receive]]
category = "Discharge-Reports"
member = "node-a"
package = "/etc/ferrofed/packages/hl7.fhir.eu.hdr-1.0.0.tgz"
dependencies = [
  # the Patient Summary's closure, as in the patient summary example, and:
  "/etc/ferrofed/packages/hl7.fhir.uv.ips-2.0.1.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-7.2.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-7.4.0.tgz",
  "/etc/ferrofed/packages/xtehr.eu.ehds.models-1.0.0.tgz",
]
template = "/etc/ferrofed/templates/ferrobridge_discharge_report.adlt"
archetypes = [
  "/etc/ferrofed/archetypes/openEHR-EHR-COMPOSITION.transfer_summary.v1.adl",
  # and every archetype the template specialises or fills a slot with
]
files = [
  "/etc/ferrofed/mappings/hdr/ferrobridge_eu_hdr.context.yml",
  # and every other file of the HDR project, in its order
]
context = "ferrobridge_eu_hdr.context"
language = "en"
territory = "NL"
```

- The Xt-EHR models package `xtehr.eu.ehds.models` 1.0.0, which the HDR
  package names, declares FHIR 5.0.0. FerroBRIDGE's validator holds it in
  the set unread, so none of its own R5 dependencies is needed.
- A document whose `Composition.type` is LOINC `34105-7` is held to every
  constraint of `bundle-eu-hdr` and `composition-eu-hdr`, and of the
  profiles its entries claim, before anything reaches a member. A document
  that breaks one is refused `422` and no member is asked.
- The document is mapped into one composition of the template, the report
  kept byte for byte in its `FEEDER_AUDIT.original_content`, and written to
  the declared member by its own `ehr_id` alone, as a
  [received patient summary](../evaluate/receiving-documents.md) is. Each
  receipt is recorded once under `Discharge-Reports` in the access log.

## A member's own discharge reports

A member can serve its discharge reports itself, as an HL7 Europe EU Health
Data API Document Access Provider. Declare an `hl7-fhir-rest` endpoint of
the member with `categories = ["Discharge-Reports"]`, the `bundle-eu-hdr`
profile (`http://hl7.eu/fhir/hdr/StructureDefinition/bundle-eu-hdr`) and
the identifier system of its `ehr_id`, as for a
[member that serves its own summary](patient-summary.md#a-member-that-serves-its-own-summary).

The gateway asks the member by its own `ehr_id` alone, with the
interactions the Document Access Provider capability statement makes
SHALL:

1. `GET {endpoint}/Patient?identifier={system}|{ehr_id}` for the logical id
   of the member's `Patient`. An empty `searchset` means the member holds no
   report of the patient, and nothing more is asked.
2. `GET {endpoint}/DocumentReference?patient={id}&type=http://loinc.org|18842-5,http://loinc.org|100719-4`
   for its discharge reports, each `next` page followed under its own base.
   The `type` names the codes of the value set your search admits, ORed
   with a comma as FHIR R4 search allows of every parameter.

The list names each `DocumentReference` the member answers as the member
holds it, with one change: each `content.attachment.url` becomes a
`Binary/{token}` the gateway signs for you, naming the member, its `ehr_id`,
the `Discharge-Reports` category and the logical id of the member's
`Binary`. Reading that URL asks the member alone for
`GET {endpoint}/Binary/{id}` and answers its bytes and media type as the
member sent them. The rules are those of
[a member's own laboratory reports](laboratory-reports.md#a-members-own-laboratory-reports):
an attachment URL that names no `Binary` under the member's endpoint, or a
`Patient` search that names more than one patient, makes the member one in
`node-error`, and a member that does not answer fails the list under
all-or-nothing, `424` or `504` naming it, and is named in an `outcome`
entry under `partial`. Each list and read is recorded under
`Discharge-Reports` in the access log, naming the member's own endpoint
and its `ehr_id` (`fhir/list/own_discharge.rs`).

## The component behind the category

The component work for discharge reports is FerroBRIDGE's, through the
published `eehrxf` crate: the
[discharge report crosswalk](https://ferrobridge.eu/docs/evaluate/category-crosswalks.html#the-discharge-report-crosswalk)
to the HL7 Europe Hospital Discharge Report profiles, and
[what the component accepts](https://ferrobridge.eu/docs/evaluate/receiving-documents.html#what-the-component-accepts)
when it receives a discharge report.
