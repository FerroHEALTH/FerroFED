<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Receiving a document

Regulation (EU) 2025/327 asks an EHR system that stores, intermediates or
gives access to health data to "be able to receive personal electronic health
data in the European electronic health record exchange format, by means of
the European interoperability software component" (Annex II, points 2.2 and
2.3). FerroFED intermediates, so it must receive. It stores nothing of its
own, so a received document goes to one member: the one the deployment
declares for the document's category, as the Federation Tier sends every new
object to one explicitly chosen node (§2.3, N23).

The interoperability component, the `eehrxf` crate published from
FerroHEALTH/FerroBRIDGE and pinned exactly, reads a received document,
checks it against its profiles and maps it into openEHR. The gateway takes
the document at `POST {fhir-base}/Bundle` and writes the composition to the
declared member, with an access record of each receipt
([The patient summary over FHIR](../integrate/patient-summary.md#receiving-a-document)).
The information sheet lists the categories a release can receive, and each
deployment's `GET {fhir-base}/metadata` the ones it receives and the ones
it has no receiving member for, with no `Bundle` create at all when it
receives none
([The categories received](information-sheet.md#the-categories-received)).
`ferrofed config check` names the same categories to the operator, each
received one with its member
([Running it](../operate/configuration.md#running-it)).

## What the component does

What the component accepts, the structural check, the full validation by
`ferrobridge-validator` and the mapping into one openEHR composition that
keeps the document in `FEEDER_AUDIT.original_content` are described where
the component is built, on
[FerroBRIDGE's receiving page](https://ferrobridge.eu/docs/evaluate/receiving-documents.html). In short:
a document is decoded strictly against FHIR R4 and held to the R4 document
rules; its subject resolves to exactly one `Patient` entry; it is validated
in full against the `Bundle` profile of its category over the package's
whole dependency closure; and no refusal quotes a value of the document.

The gateway adds what is its own:

- each `[[fhir.receive]]` names the package and, in `dependencies`, every
  package of its dependency closure, which the validator refuses to load
  short of (decision A106);
- an issue of severity `error` or `fatal` refuses the document with `422`
  and an `OperationOutcome` of every such issue, before any member is asked,
  and each warning, and every constraint the validator met and did not
  evaluate as a `warning` of type `not-supported`, is returned in the
  outcome of the write;
- a `required` binding the packages cannot decide is asked of the
  `[eehrxf.terminology]` server, and a server that refuses or does not
  answer refuses the document with `502`; with no server configured the
  binding is listed as not evaluated, and a coding that carries no display
  refuses the document ([Terminology](../integrate/terminology.md));
- the FHIRconnect mapping files the component runs are the deployment's.

## Written to the declared member

The federation half takes the composition and the document's patient. It
resolves the patient from every identifier of the document's `Patient` in a
namespace `fhir.receive_namespaces` names, at the declared member alone, and
every one must name the same `ehr_id` there: identifiers that name two
patients, or a patient the member holds no EHR for, are refused before
anything is sent. A caller whose grant is confined to one patient writes
only into that patient's EHR. The composition then goes to the member as an
ITS-REST `composition_create`, with no patient identifier in the request
path, query string or headers (§5.4.1, N33); the body keeps the received
document, which is the clinical content the client sent. A member that
refuses the composition or does not answer is an error that names it, never
a success.

## A laboratory report

A release receives three categories as documents: patient summaries,
laboratory reports, Art 14(1)(e), and discharge reports, Art 14(1)(f). A laboratory report is held to the HL7
Europe Laboratory Report 2.0.0 `Bundle-eu-lab` and `Composition-eu-lab`
profiles, which a document whose `Composition.type` is LOINC `11502-2`
names, and mapped by the laboratory mapping the component ships over the
`KDS_Laborbericht` template into one composition. Declare the member that
stores laboratory reports with its own `[[fhir.receive]]`, naming the
package `hl7.fhir.eu.laboratory` 2.0.0 and its dependency closure, the
template and the mapping files, which `eehrxf` writes out of its
`mappings::LABORATORY` project:

```toml
[[fhir.receive]]
category = "Laboratory-Reports"
member = "node-a"
package = "/etc/ferrofed/packages/hl7.fhir.eu.laboratory-2.0.0.tgz"
dependencies = [                             # the package's dependency closure
  "/etc/ferrofed/packages/hl7.fhir.r4.core-4.0.1.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-7.1.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.extensions.r4-5.2.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.xver-r5.r4-0.1.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.extensions.r4-1.3.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.base-2.0.0.tgz",
]
template = "/etc/ferrofed/mappings/laboratory/KDS_Laborbericht.opt"
files = [                                    # the files of eehrxf's laboratory project, in its order
  "/etc/ferrofed/mappings/laboratory/ferrobridge_eu_laboratory.context.yml",
]
context = "ferrobridge_eu_laboratory.context"
language = "en"
territory = "NL"
```

List every file of the project in `files`, in the order the project names
them; the example shows the first. The composition carries each result's
value with its unit, the report's code, status, time and conclusion and
each specimen. Since `eehrxf` 0.1.149 it also carries a result's normal
reference range, as the `normal_range` of its quantity, and the order
filler's identifier, as the order the template holds. Since `eehrxf`
0.1.183 a typed range, one whose `Observation.referenceRange.type` is set,
is carried as one of the quantity's `other_reference_ranges`, with its type
as the range's meaning
([FerroHEALTH/FerroBRIDGE#1694](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1694)).
A range text, `Observation.basedOn` and an order named by reference alone
are declared lost, and the answer's `OperationOutcome` names each loss
([FerroHEALTH/FerroBRIDGE#1481](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1481)).
The member's record holds those only in the original document the
composition keeps. Hazard I-16 of the [clinical safety risk
file](clinical-safety.md) stays open at S4, P3: a reader of the member's
record is not told of those losses, so they still arise in normal use.

## An imaging or a discharge report

Since `eehrxf` 0.1.170 the component maps a received medical imaging
report, Art 14(1)(d), and a discharge report, Art 14(1)(f), the same way.
The gateway receives both
([#1716](https://github.com/FerroHEALTH/FerroFED/issues/1716)). An imaging report
is held to the HL7 Europe Imaging Report 1.0.0-ballot `BundleReportEuImaging`
and `CompositionEuImaging` profiles, which a document whose
`Composition.category` is `Medical-Imaging` names, and mapped by the
component's `mappings::IMAGING` project (context
`ferrobridge_eu_imaging.context`). A discharge report is held to the HL7
Europe Hospital Discharge Report 1.0.0 `bundle-eu-hdr` and
`composition-eu-hdr` profiles, which a document whose `Composition.type` is
LOINC `34105-7` names, and mapped by the `mappings::HDR` project (context
`ferrobridge_eu_hdr.context`). Declare each category's member with its own
`[[fhir.receive]]`, as the laboratory example above shows: the category's
package and its dependency closure, the template the project maps over and
every file of the project in its order. The discharge project maps
FerroBRIDGE's ADL 2 template, so its entry names the `.adlt` template with
every archetype it holds in `archetypes`
([Discharge reports the gateway receives](../integrate/discharge-reports.md#discharge-reports-the-gateway-receives)),
and so does the imaging project's
([Imaging reports the gateway receives](../integrate/imaging-reports.md#imaging-reports-the-gateway-receives)).

## Inserted by the person

Regulation (EU) 2025/327 Art 5 asks that information a person, or their
representative of Art 4(2), inserts in the person's EHR "be clearly
distinguishable as having been inserted by the natural person or by his or
her representative". When the caller is a person acting under their own grant,
a token whose `sub` names the person and whose `patient/` scopes are
confined to them ([Patient grants](../operate/patient-grants.md)),
the gateway has the component mark the composition before it is sent: every entry, in
the composition and in each of its sections, names the record's subject
as its `provider`. The openEHR RM defines `ENTRY.provider` as "the agent
who provided the information", which "is usually the patient or the
clinician" (RM 1.1.0, EHR Information Model, "Entry and its Subclasses"),
and refers to the subject of the record with a `PARTY_SELF` (Common
Information Model, "PARTY_SELF and Referring to the Patient from the EHR").
The mark is a `PARTY_SELF` with no `external_ref`, so it names no
identifier of the person. A provider the mapping wrote is replaced; the
document as received stays in the original content.

A composition with no entry, or with a `GENERIC_ENTRY`, which has no
provider, cannot carry the mark, so a person's document that maps to one is
refused `422` with nothing sent. A professional's document is written as
before, with no provider set. The access record of a person's receipt
carries the `ehds-subject-inserted` entity ([Information a person
inserted](../operate/access-log.md#information-a-person-inserted)).

A proxy or a legal representative who acts for the person under the
person's grant is told apart only when the token says a representative
acts: its issuer declares that the RFC 8693 `act` claim of its patient
tokens names one, and the token carries that claim
([Patient grants](../operate/patient-grants.md)). Every
entry then names the representative as its `provider`, a
`PARTY_IDENTIFIED`, which the Common Information Model uses for every party
other than the subject of the record ("Referring to Demographic
Entities"), never a `PARTY_SELF`. Its one identifier is the `act` claim's
`sub`, issued by the claim's `iss` or the token's own; it carries nothing
else the token does not state. A token that names the representative by
nothing, by its own `sub` or by an identifier of the document's patient is
refused `403` with nothing sent, so the provider names nothing of the
person. The access record carries the `ehds-representative-inserted`
entity in place of `ehds-subject-inserted`. A token that cannot say a
representative acts, because its issuer declares no `act` claim or the
proxy service issues tokens in the person's own name, is read as the
person's own, and the gateway cannot mark the representative.

The component only creates compositions, so it never changes an entry a
professional wrote. What a person changes over ITS-REST is the member's to
refuse ([Patient grants](../operate/patient-grants.md)).

## Hazards

The [clinical safety risk file](clinical-safety.md) lists the receive path
as I-10 (a document filed under the wrong member or patient), whose
controls are built, and I-11 (a document stored with content lost), whose
controls include the full validation above
([#808](https://github.com/FerroHEALTH/FerroFED/issues/808)). The safety
review of 2026-10-25 closed both at S4, P1
([#1327](https://github.com/FerroHEALTH/FerroFED/issues/1327)). A received laboratory report adds I-16 (a result the
record does not hold whole), open at S4, P3. A received discharge report
adds I-21, open at S4, P3: its diagnoses, procedures and discharge
medicines reach the member's composition with every element the mapping
reads, and a diagnosis's severity and certainty are relayed as not
carried, but its hospital course and every section the template holds no
place for reach the member's record only in the original document, and
the answer does not name them
([FerroHEALTH/FerroBRIDGE#1820](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1820)).
A received imaging report adds I-22, open at S4, P3: its study and
impression reach the member's composition, but its report identifier, its
performing organization and each section's narrative reach the member's
record only in the original document, undeclared
([FerroHEALTH/FerroBRIDGE#1827](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1827),
[#1716](https://github.com/FerroHEALTH/FerroFED/issues/1716)).
