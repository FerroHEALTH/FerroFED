<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Receiving a document over FHIR

**Audience:** integrators who send a document in the European exchange format, and operators who declare which member receives each category. [Receiving a document](../evaluate/receiving-documents.md) records what Regulation (EU) 2025/327 asks of it.

## Receiving a document

Annex II 2.2 and 2.3 ask an EHR system to be able to receive personal
electronic health data in the exchange format. Send a document `Bundle` to
the face's `Bundle` end-point (FHIR R4 documents §3.3.4):

```http
POST {fhir-base}/Bundle
Content-Type: application/fhir+json
Authorization: Bearer <token>

{"resourceType":"Bundle","type":"document", ...}
```

The gateway stores nothing itself. It writes the document as one openEHR
composition to the one member the deployment declares for the document's
category (Federation Tier §2.3, N23):

1. The document is read under the FHIR R4 document rules, and its category
   is the one whose HL7 Europe document profile fixes a coding of its
   `Composition.type`: LOINC `60591-5` for a patient summary. A category no
   member receives is a `422` that names it.
2. It is validated in full by FerroBRIDGE's validator against the
   category's `Bundle` profile, over the package and the dependency closure
   the deployment supplies: every constraint of that profile, of the
   profile each entry's slice names and of each profile an entry claims,
   invariants and terminology bindings included
   ([Receiving a document](../evaluate/receiving-documents.md#what-the-component-does)).
   A document with an error is a `422` with one issue per error, located by
   its FHIRPath. A warning, and each constraint the validator cannot
   evaluate, is listed as a `warning` in the answer, never passed silently.
3. Every coding that carries no `display` is given one by the terminology
   server's `$lookup`, because the openEHR side needs the text of every
   coded term. With no terminology server configured, such a coding is a
   `422` naming where it is.
4. It is mapped into one composition by the category's FHIRconnect mapping.
   The composition keeps the whole document, as it was received, in its
   `FEEDER_AUDIT.original_content`. A document names no openEHR
   `COMPOSITION.category`, so the composition takes the one the category's
   template decides: its assumed value, or its one admitted code, or `event`
   (openehr `433`) when the template leaves the category open. A template
   that admits several category codes and assumes none refuses the mapping,
   naming `<root>/category|code`; give the template's `category` an assumed
   value ([The template's category](#the-templates-category)).
5. The patient is resolved from every identifier of the document's `Patient`
   in a namespace `fhir.receive_namespaces` names, at the receiving member
   alone (§5.2). Every one must name the same `ehr_id` there. A patient the
   member holds no EHR for, or identifiers that name two patients, are a
   `422`, and nothing is sent to any member.
6. The composition is committed with ITS-REST `composition_create` to the
   member, located by its own `ehr_id`. No patient identifier is in the path,
   the query string or the headers (§5.4.1, N33). The body carries the
   received document as the clinical content you sent.

A written document answers `200` with an `OperationOutcome` whose first issue
names the category, the member, the endpoint and the new version's uid. A
member that refuses the composition is a `502` that names its status, one
that does not answer in time is a `504`, and identity services that cannot
answer give a `424`. A terminology server that refuses or does not answer
is a `502` that names its status. None of them is ever a success.

Receiving takes the SMART on openEHR create permission on compositions of
every template, `user/composition-*.c`, because the template follows from
the document. A read scope is refused. A `patient/` grant, where the issuer
is bound to a member, writes only into its own patient's EHR. Every receipt
that reached the member writes one access record, an action `C` with the
document's category by construction. A receipt whose record has no place in
the spool is refused `503` before the member is asked, so sending it again
writes it once ([Failing closed](../operate/access-log.md#failing-closed)).

The capability statement, `GET {fhir-base}/metadata`, declares the
`create` on `Bundle` only when a member receives a category, as it lists a
prescription resource only when a member serves it. Its documentation then
names the categories the deployment receives and those it has no receiving
member for, by their HL7 Europe codes, and its supported profiles are the `Bundle` profiles of the
categories received. `ferrofed config check` prints the same categories,
each received one with its member
([Running it](../operate/configuration.md#running-it)).

### The template's category

`COMPOSITION.category` is mandatory in the openEHR RM, and a received
document never names one. The mapping takes it from the template's
constraint on `/category`. When that constraint admits more than one code,
`persistent` (`431`) and `event` (`433`) for example, the template must say
which one a mapped composition carries, with an assumed value on the
`C_CODE_PHRASE` (AOM 1.4 `assumed_value`):

```xml
<children xsi:type="C_CODE_PHRASE">
  <rm_type_name>CODE_PHRASE</rm_type_name>
  <!-- occurrences and node_id as the template has them -->
  <assumed_value>
    <terminology_id><value>openehr</value></terminology_id>
    <code_string>433</code_string>
  </assumed_value>
  <terminology_id><value>openehr</value></terminology_id>
  <code_list>431</code_list>
  <code_list>433</code_list>
</children>
```

Without one, every document of that category is refused: the mapping fails
with `MissingRequiredSuffix` on `<root>/category|code`, and the document is
a `422` with the issue code `processing`, with nothing sent to any member. A template that admits
one code, or leaves the category open, needs no change.
