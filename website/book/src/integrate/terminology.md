<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Terminology on the FHIR face

The [patient summary](patient-summary.md) the FHIR face writes, and a
document it receives, carry their codes by the rules below.

Regulation (EU) 2025/327 Art 2(2)(f) defines interoperability as an
exchange "without changing the content of the data". The interoperability
component therefore sends a code as it was recorded, and adds a coding
beside it only where the coding means the same thing:

- A code in a preferred code system passes as recorded, displayed with the
  text the record holds.
- An openEHR `TERM_MAPPING` whose `match` is `=`, an equivalent term, adds a
  coding after the original.
- A mapping whose `match` is `>` (broader), `<` (narrower) or `?` (unknown)
  adds no coding, because a FHIR coding states the concept itself. It is
  written into the narrative.
- The text of every coded item is in the narrative, as Art 10(3) of the
  eHealth Network Guidelines on Patient Summary asks.
- A code of the archetype-local terminology (`local`, an `at` or `ac`
  code) is displayed with the text the operational template defines for it
  in the archetype the item sits in, in the language asked. The coding is
  kept as recorded, and the term's text opens the narrative. A template that
  holds the term only in another language leaves the code to Art 10(7);
  no other language is substituted. A local code the archetype does not
  define is refused, because the item does not conform to its template.
- Any other code that is in no preferred system follows Art 10(7) of those
  guidelines: it is "transferred encoded, preferably displayed in English,
  and/or in narrative form". The original coding is kept with its display,
  and its text is in the narrative.

The preferred code systems are those §4 of the guidelines names for its
data elements. The component ships thirteen of them, and the rules prefer
them by default. Twelve are the system URIs the MyHealth@EU Master Value
Sets Catalogue 9.1.0 includes them by: SNOMED CT, ICD-10, Orphacode, ATC,
the EDQM Standard Terms, UCUM, LOINC, ISO 3166, and HL7 Administrative
Gender, RoleClass, RoleCode and ObservationCategoryCodes. The catalogue
includes no NPU, so NPU is `http://npu-terminology.org`, the URI the HL7
Europe Laboratory Report 2.0.0 includes it by.

EMDN is not shipped. The catalogue's medical device value set includes one
system by an OID, `urn:oid:1.3.6.1.4.1.12559.11.10.1.3.1.44.6`, but no
vendored text says which code system that OID is, and no vendored package
names a URI for EMDN. EMDN is added once a MyHealth@EU catalogue release,
an HL7 Europe guide or the publisher of EMDN names its system URI. IDMP is
not shipped either, because §4 lists it for use when available. A caller
may name its own set instead.

A coding carries the version of its terminology, the part of the openEHR
terminology id in parentheses, as its `Coding.version` only in the form
that code system's versions take:

- SNOMED CT names a version by a URI of its edition, such as
  `http://snomed.info/sct/900000000000207008/version/20260201`, as the
  vendored HL7 France Core 2.2.0 and HL7 Europe Laboratory Report 2.0.0
  write it. A version in that form passes as recorded. Any other, such as
  `SNOMED-CT(20260201)`, names no edition, so the coding carries no
  version: the edition cannot be inferred without changing the data.
- HL7 Administrative Gender, RoleClass, RoleCode and
  ObservationCategoryCodes name a version by dot-separated digits, such as
  `4.0.0`, as the MyHealth@EU catalogue includes them. A version in that
  form passes as recorded, and any other is left out.
- For ICD-10, Orphacode, ATC, the EDQM Standard Terms, UCUM, LOINC, NPU and
  ISO 3166, no vendored package shows how the system writes a version, so
  the version passes as recorded. The form of each is added once a
  vendored source shows it.

These rules are built into the component. The summary the face writes
carries the codings its FHIRconnect mappings write: the original code as
recorded, and a coding after it for each `TERM_MAPPING` whose `match` is
`'='`. A mapping whose `match` is `'>'`, `'<'` or `'?'` is written as no
coding, and its section's narrative carries it as one line, such as
`AllergyIntolerance.code: broader term SNOMED-CT 762952008, from Hospital A.`
([#926](https://github.com/FerroHEALTH/FerroFED/issues/926)). A code in no
preferred system is translated through the configured terminology server,
as below, and with none is kept as recorded (Art 10(7))
([#1213](https://github.com/FerroHEALTH/FerroFED/issues/1213)).

## The terminology server

A FHIR R4 terminology server is optional. When `[eehrxf.terminology]` names
one, the component asks it for three operations:

- `ConceptMap/$translate`, for a code in no preferred system. Only a match
  whose equivalence is `equivalent` or `equal` becomes a coding; a `wider`,
  `narrower`, `inexact` or other match never does. In the patient summary,
  each `CodeableConcept` of a mapped resource none of whose codings is in a
  preferred system is asked once per coding and preferred system, and every
  accepted match is added after the original coding, which stays as
  recorded. A translation the server refuses or does not answer fails the
  summary with `500`, and is never taken as no translation. A bare `Coding`
  holds one code, so it is kept as recorded.
- `CodeSystem/$lookup`, for the display of a received coding that carries
  none.
- `ValueSet/$validate-code`, for a `required` binding whose value set no
  vendored package expands.

With no server configured, nothing is translated and a code in no preferred
system follows Art 10(7). A received coding with no display is refused, and
a `required` binding no vendored expansion decides is listed as not
evaluated. A server that refuses an operation, or does not answer within
`timeout_ms`, is an error that names the operation and the server's status,
never a coding left out or a binding taken as held.

FerroTERM is the terminology server of the Ferro family. The gateway reaches
it, or any other FHIR R4 terminology server, over HTTP, and links no part of
it. The EDQM Standard Terms are not shipped with FerroFED: a deployment loads
them into its terminology server under its own licence with the EDQM.

## Required bindings

A `required` binding means the concept "SHALL be from the specified value
set" (FHIR R4). The validator holds each coded value that a binding of the
document's profiles covers to its value set, read from the packages the
deployment supplies: the FHIR R4 core package and HL7 Terminology among
them, so the value sets of FHIR itself are decided offline. A concept
outside a `required` value set is a `422` finding, and one outside an
`extensible` value set a `warning`. A `CodeableConcept` holds when one of
its codings is in the value set.

A value set no package enumerates, such as one over SNOMED CT or LOINC, is
asked of the terminology server's `ValueSet/$validate-code`. A server that
refuses or does not answer the check of a `required` binding refuses the
document with `502`, since the gateway cannot say it conforms. Without a
terminology server, such a binding is listed as a `warning` that names the
value set as not evaluated.
