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

1. An object that repeats a name is refused before anything reads the
   text, because two JSON readers may keep different values for it (RFC
   8259, §4).
2. The text is decoded against the R4 definitions of `Bundle` and of every
   resource it carries. An unknown property, a value of the wrong type, a
   `null` or an empty object or array, and an invalid primitive are refused,
   with the element path where they were found, and so is a resource of a
   type R4 does not define, anywhere in the document. The decoded document
   must encode back to the JSON it was read from. Every later rule, the
   profile check and the mapping read that one decoded document.
3. The `Bundle` is a `document`, and meets the R4 rules for one: an
   `identifier` with a system and a value (`bdl-9`), a `timestamp`
   (`bdl-10`), `fullUrl`s given once (`bdl-7`), none version specific
   (`bdl-8`), each REST-style one ending with its resource's type and id,
   and a `Composition` as its first entry (`bdl-11`).
4. The `Composition.subject` names one `Patient` entry of the same `Bundle`,
   by its `fullUrl`, or by a relative `Patient/<id>` resolved against the
   server base of the composition's own `fullUrl`. Every reference in the
   document is found by its R4 type, at any depth: in backbone elements,
   contained resources and extensions, and resolved by one resolver,
   `receive::reference::resolve`, which answers exactly one entry or a
   typed refusal. It reads a reference and a `fullUrl` in one canonical
   spelling only: a relative `Type/id`, an `http` or `https` URL with a
   lower-case host, `urn:uuid:` with a lower-case UUID, `urn:oid:`, a
   `_history` version matched on the entry's `meta.versionId`, and a `#id`
   into the resource's own `contained`. Whitespace, a query string,
   percent-encoding, a trailing or doubled slash, a dot segment and a
   resource type in another case are refused, so no reader can take one of
   them for an entry the resolver would not. A `type` or an `identifier`
   beside the reference must agree with its target. A `subject`, `patient`,
   `beneficiary` or `for` must name that same entry by reference. Any other
   reference may name it, an entry of another type, a contained resource
   by `#id`, or a resource outside the document whose path names a type
   other than `Patient`; one that could name another patient (a path or a
   `type` that says `Patient`, or a URN no entry carries) is refused, and so
   is a `type` that disagrees with the reference's target. A second
   `Patient` entry and a contained `Patient` are refused, and so is an
   element the R4 element table does not describe. A document is
   registered under the identification data of the one person it is about
   (Art 13(3)), so a document that could be read as about two is refused
   whole.

The document's text is kept byte for byte. No refusal quotes a value of the
document, so a patient identifier never reaches an error message or a log
line through one. Whether the caller may write for that patient, and whether
the member's `ehr_id` is that patient's, the federation half decides before
it sends anything
([#802](https://github.com/FerroHEALTH/FerroFED/issues/802)).

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
  An entry of a resource type no slice of the `Bundle` profile names is
  refused, although the profile's slicing is open, so no entry passes
  unread;
- the invariants of one form: every entry of the listed types carries a
  reference at one element, as the EPS `eps-bundle-subject-ref` and
  `eps-bundle-patient-ref` require. Every other invariant is listed as not
  evaluated.

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
over the occurrences the other discriminators admit. Other invariants, terminology
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
