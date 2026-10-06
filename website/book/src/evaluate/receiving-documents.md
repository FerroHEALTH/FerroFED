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

The receive path comes in three parts, and the first is built: the
interoperability component, `crates/eehrxf`, reads a received document,
checks it against its profiles and maps it into openEHR. The gateway does
not serve it yet. Writing the composition to the declared member is planned
([#802](https://github.com/FerroHEALTH/FerroFED/issues/802)), and so is the
interaction a client sends the document with, its access record and the
information sheet's list of the categories received
([#803](https://github.com/FerroHEALTH/FerroFED/issues/803)).

## What the component accepts

A received document is a FHIR R4 `Bundle` in JSON. The component reads it in
this order and refuses it at the first rule it breaks:

1. The text is decoded against the R4 definitions of `Bundle` and of every
   resource it carries. An unknown property, a value of the wrong type or an
   invalid primitive is refused, with the element path where it was found.
2. The `Bundle` is a `document`, and meets the R4 rules for one: an
   `identifier` with a system and a value (`bdl-9`), a `timestamp`
   (`bdl-10`), and a `Composition` as its first entry (`bdl-11`).
3. The `Composition.subject` names one `Patient` entry of the same `Bundle`,
   by its `fullUrl` or by a relative `Patient/<id>` on the same server base.
   A document is registered under the patient's identification data (Art
   13(3)), so a document that does not say which of its entries is the
   patient is refused.

The document's text is kept byte for byte. No refusal quotes a value of the
document, so a patient identifier never reaches an error message or a log
line through one.

## The check against the profiles

The document is then checked against the HL7 Europe profiles of its
category, read from the vendored package: for a patient summary,
`bundle-eu-eps` and `composition-eu-eps` of `hl7.fhir.eu.eps` 1.0.0-ballot.
At every occurrence of every element the profile's snapshot lists, the check
holds:

- the cardinality, counted per occurrence of the parent element;
- the value a `fixed[x]` or `pattern[x]` requires, for a `CodeableConcept`,
  a `Coding` or a string-valued primitive;
- the slices of a sliced element, told apart by `value`, `pattern`,
  `exists` and `type` discriminators. Each slice is held to its own
  cardinality, and a `closed` slicing refuses an occurrence no slice admits.
  The five sections the EPS composition requires are slices of this kind.

Every finding is reported with the profile, the element id and the place in
the document, such as `Composition.section[2]`. None is dropped.

The check says what it cannot evaluate, and lists it beside a passing
result:

- a `profile` discriminator, such as the one that tells the EPS `Bundle`
  entries apart by the profile of their resource;
- a discriminator path through `resolve()`, such as the one that tells a
  section's entries apart by the type of the resource they reference;
- the order of an ordered slicing;
- a `fixed[x]` or `pattern[x]` form it does not read, by its key.

A slice such a discriminator tells apart is still held to its lower bound
over the occurrences the other discriminators admit. Invariants, terminology
bindings and the profiles of the other entries are not checked yet
([#808](https://github.com/FerroHEALTH/FerroFED/issues/808)). The two
example documents the EPS package publishes pass the check.

## Mapped into openEHR, with the original kept

The component maps the document into one openEHR composition through the
FHIRconnect mapping files the deployment supplies, run in process by
FerroBRIDGE's `fhirconnect` engine. The whole document is the input of
FHIRconnect's `$toopenehr`, so the context that maps it is the one whose
profile the document's resources claim, and a mapping may follow the
document's references into its other entries. A document no context maps,
or one with two resources of the type a context maps, is refused with the
engine's reason.

The composition keeps the document itself. The openEHR RM lets
`FEEDER_AUDIT.original_content` carry the original content inline, and puts
one at the composition to establish the equivalence between the whole
composition and the whole document (RM 1.1.0, Common Information Model,
"Original Content"). The component sets it to a `DV_PARSABLE` holding the
document's text with the formalism `application/fhir+json`, beside the
engine's own audit of the system the content passed through. What the
mapping does not carry into structured content is therefore still in the
record, and what the run declared lost comes back beside the composition as
an `OperationOutcome`.

The federation half, planned in
[#802](https://github.com/FerroHEALTH/FerroFED/issues/802), takes the
composition and the document's patient: it resolves the patient to the
declared member's own `ehr_id` and sends the composition there as an ITS-REST
`composition_create`, with no patient identifier in the request path, query
string or headers (§5.4.1, N33).

## Hazards

The [clinical safety risk file](clinical-safety.md) lists the receive path
as I-10 (a document filed under the wrong member or patient) and I-11 (a
document stored with content lost). Both stay open until the gateway writes
a received document and validates it in full.
