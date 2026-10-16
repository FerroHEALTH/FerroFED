<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The patient summary over FHIR

Regulation (EU) 2025/327 Annex II 2.1 asks an EHR system for "an interface
enabling access to the personal electronic health data processed by it in
the European electronic health record exchange format". FerroFED serves that
interface as a FHIR R4 face on a base of its own, `{fhir-base}`, beside the
ITS-REST face at `{base}`. No ITS-REST path changes: `{base}` stays a
conformant ITS-REST surface (Federation Tier §4.1, N1, N28).

The face answers five requests today:

| Request | Answer |
|---|---|
| `GET {fhir-base}/metadata` | the face's `CapabilityStatement` |
| `GET` or `POST {fhir-base}/Patient/$summary` | the patient summary, an HL7 Europe Patient Summary document `Bundle` |
| `POST {fhir-base}/Bundle` | a document received in the exchange format, written to the member its category is declared for |
| `GET {fhir-base}/DocumentReference` | the document list: the summary the gateway writes on request and each member's own one ([The document list](#the-document-list)) |
| `GET {fhir-base}/Binary/{token}` | a member's own document, at the URL the document list names |

The gate refuses every other path under `{fhir-base}`, and every other
method, with a `403`: a variant of a path, in another letter case, with a
percent-encoded character or a trailing or doubled slash, is refused, never
served.

## Asking for a summary

The request is the International Patient Summary 2.0.0 `$summary` operation
on `Patient`, at the type level. Name the patient by an identifier with its
system:

```http
GET {fhir-base}/Patient/$summary?identifier=urn:oid:2.999.1|SYNTHETIC-1
Authorization: Bearer <token>
```

or send the same parameters as a FHIR `Parameters` body, with
`Content-Type: application/fhir+json`:

```http
POST {fhir-base}/Patient/$summary
Content-Type: application/fhir+json

{"resourceType":"Parameters","parameter":[
  {"name":"identifier","valueString":"urn:oid:2.999.1|SYNTHETIC-1"}]}
```

- `identifier` is required, once, as `system|value`. The system is the
  namespace the gateway resolves the patient in (§5.2), as it is for an AQL
  query's `external_ref/namespace`.
- `profile`, when sent, is
  `http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps`, with or
  without `|1.0.0-ballot`.
- `_format`, when sent, is `json`, `application/json` or
  `application/fhir+json`.
- Any other parameter is a `400`. The face never searches by demographics:
  a national connector identifies the patient against its own identity
  services first and asks by the confirmed identifier.
- A request by the patient's logical id, `Patient/{id}/$summary`, is
  refused: the face holds no `Patient` resource.

## What the gateway does

The summary is built from the gateway's own section queries, the eleven
stored queries under `eu.ferrofed.eehrxf`
([the gateway's own queries](stored-queries.md#the-gateways-own-queries)):

1. The header is asked of the demographics binding, the PDQm Supplier of
   `[pdqm]`, by the identifier you sent, before any member is asked. The
   members federate no demographics (Federation Tier §2.3, N32), and nothing
   of the header reaches a node.
2. Every section query is read as the façade reads a stored query, the
   patient bound through `$patient` and `$namespace` alone.
3. The patient is localized, checked against the consent pre-filter and
   resolved once, at every member (§5.2, §14). Each section query is then
   planned over that one resolution, so the identity services are asked once
   per summary.
4. Every section query goes to every member that holds the patient, scoped
   to that member's own `ehr_id`, through the same rewrite and outbound
   identifier-hygiene gate as any query. No patient identifier reaches a
   node (§5.4.1, N33). The queries run together under one budget (§11.5).
5. Each composition a section query selected is mapped by every FHIRconnect
   mapping the deployment supplies for that section and the composition's
   template. A composition no mapping covers is counted in the section's
   narrative, never dropped in silence.
6. The document is written: the composition, the patient, the gateway's
   `Device` and the operator's `Organization` as its authors, each member
   whose data a section holds as an author of that section, one `Provenance`
   per mapped composition naming the composition and the member, and every
   mapped resource.

The section queries run from the code that defines them, so the face works
whether or not the deployment sets `[stored_queries]`. The text is the one
the registry lists at version `1.0.0`.

### A member that serves its own summary

A member the registry declares in the `native` mode for patient summaries
([a member's own exchange-format endpoint](../operate/registry.md#a-members-own-exchange-format-endpoint))
is resolved with every other member, then left out of the section queries.
The gateway asks its own endpoint instead, at the same time and under the
same budget:

```text
GET {member-fhir-base}/Patient/$summary?identifier=<ehr_id_system>|<ehr_id>&profile=http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps
Accept: application/fhir+json
```

The request names the member's own `ehr_id` and nothing else about the
patient (§5.4.1, N33). It passes the outbound identifier-hygiene gate, and
carries the onward credential and the signed caller identity of the
member's `openehr-rest-query` endpoint. The member's document comes back as
it sent it, byte for byte. The gateway's own document maps none of that
member's data, and every section names it: "Not mapped here: the records of
the member, which serves its own patient summary." A section with no entry
is then `unavailable`, never `nilknown`, because the gateway did not read
that member's data. The document list names the member's own document,
which a client reads through the gateway
([The document list](#the-document-list)).

A member whose own endpoint answers `404` holds no summary of the patient,
and the sections say so. Any other answer, or none, counts as a member that
did not answer, as below.

## The document

The answer is a `Bundle` of type `document`, `application/fhir+json`, that
claims the `bundle-eu-eps` profile. Its first entry is the `Composition`, and
every entry is named under the absolute URL of `{fhir-base}`.

- The five sections the EPS composition requires are always present:
  problems, allergies and intolerances, medication summary, procedures, and
  medical devices. The other six sections the section queries feed follow.
- Every section has a generated narrative that says it is not exhaustive,
  names every member that gave no answer, and counts what no mapping covers.
- A section with no entry carries the empty reason `nilknown` only when
  every member answered and none holds content for it, and `unavailable`
  otherwise.
- Each member's data are listed as mapped, beside every other member's,
  never merged with them.
- The `Patient` carries the identifier you asked by, and the header the
  demographics binding holds (eHN PS A.1.1, A.1.2): the names, the birth
  date, the administrative gender, the addresses and the phone numbers and
  email addresses, each as the PDQm Supplier gives it. An element the
  Supplier does not hold is left out, never filled in; the birth date, which
  the EPS `Patient` requires, then carries a `data-absent-reason` of
  `unknown`. The country of affiliation, the preferred professional, the
  contact person and the insurance (A.1.1.6, A.1.2.2, A.1.2.3, A.1.3) are
  not in the header.

The tests hold every document the face writes to a structural check against
the vendored `bundle-eu-eps`, `composition-eu-eps` and `patient-eu-eps`
profiles, and CI runs HL7's FHIR Validator over every document those tests
write and fails on an error
([#688](https://github.com/FerroHEALTH/FerroFED/issues/688)).

## The document list

A `DocumentReference` search names every patient summary that exists for a
patient: the one the gateway writes on request, and the one each member in
the `native` mode serves itself. It is the search of the HL7 Europe EU
Health Data API Document Access Provider, with its `patient.identifier` and
`type` parameters:

```http
GET {fhir-base}/DocumentReference?patient.identifier=urn:oid:2.999.1|SYNTHETIC-1&type=http://loinc.org|60591-5
Authorization: Bearer <token>
```

- `patient.identifier` is required, once, as `system|value`, one patient
  alone. The patient is resolved as for a summary (§5.2).
- `type`, when sent, is a token: `60591-5`, `http://loinc.org|60591-5` or
  `http://loinc.org|`. A `type` that admits no patient summary matches
  nothing: the answer is an empty `searchset`, and nothing is resolved or
  sent.
- `_format`, when sent, is `json`, `application/json` or
  `application/fhir+json`. Any other parameter or modifier is a `400`.

The answer is a `Bundle` of type `searchset`, each entry a `DocumentReference`
that claims the `EehrxfMhdDocumentReference` profile, typed LOINC `60591-5`
in the `Patient-Summaries` category, its `subject` the identifier you sent:

- The summary the gateway writes on request, its custodian the operator of
  `[fhir.operator]`. The gateway does not write it to list it: its
  attachment points at the request that writes it, relative to
  `{fhir-base}`, `Patient/$summary?identifier=<system>|<value>`.
- One entry per member in the `native` mode whose own endpoint answered
  with a document, its custodian the member's organisation. Its
  `attachment.url` is `Binary/{token}`, relative to `{fhir-base}`, where the
  gateway serves the document ([Reading a member's document](#reading-a-members-document)),
  never a URL of the member's. `contentType` is the media type the member
  answered with (`application/octet-stream` when it named none), `hash` the
  base64 of the SHA-1 digest of the bytes the list fetched, and `size` their
  number. Its `masterIdentifier` is the document's own
  `Bundle.identifier`.

To list, the gateway asks each `native` member's own endpoint, by the
member's own `ehr_id` alone, as for a summary
([A member that serves its own summary](#a-member-that-serves-its-own-summary)),
under the same budget, onward credential and signed caller identity. It runs
no section query and maps nothing. It does not ask the demographics binding
for the summary header either: the summary written on request is listed
whenever the patient resolves at a member, and `Patient/$summary` can still
refuse it later when the header cannot be written. A member whose own endpoint answers
`404` holds no summary and has no entry. A patient no member holds matches
nothing, an empty `searchset`, as does a patient whose members may not
disclose a restriction.

A `native` member that does not answer fails the list under
all-or-nothing, `424` or `504`, with an `OperationOutcome` naming it, as for
a summary. Under `partial`, the list answers with the entries it has, and
an `OperationOutcome` entry with `search.mode` `outcome` names each member
that did not answer and its §11.1 status, so the list is never presented as
whole.

A member the identifier cross-reference could not answer for counts as a
member that did not answer, named `not-resolved`. When that leaves no
member the patient resolved at, the list fails with `424` under
all-or-nothing, and under `partial` it holds an `outcome` entry for each
member and no summary written on request. It is never the empty
`searchset` of a patient no member holds.

`metadata` declares the search: the `DocumentReference` resource with the
`search-type` interaction, its supported profile and the two parameters,
and the `Binary` read.

### Reading a member's document

`GET {fhir-base}/Binary/{token}` answers a member's document as the member
sends it, with the member's media type. The token is a compact JWS the
gateway signs with its `[signing]` key, of its own JOSE type, so the gateway
takes no other token it signs for one. It is valid for
`fhir.document_url_lifetime_secs`, 300 seconds unless set, names the
gateway's FHIR base as its audience, and carries the member's endpoints,
the member's own `ehr_id`, the category and profile, and the caller it was
listed for. It carries no patient identifier (§5.4.1, N33).

The read takes the scopes of a summary. The gateway verifies the token and
holds the caller to the one it names; an expired, altered or foreign token,
or one listed for another caller, is a `404` that names nothing. It then
asks the member again, by its own `ehr_id` alone, as the list did, and
stores nothing. A member that does not answer is a `424` or `504` naming
it, as for a summary, and one that answers `404` holds the document no
longer. A member that writes its summary anew on every request answers
bytes whose hash differs from the one the list gave. Every read that
reached a member writes one access record, naming the member's endpoint and
the `ehr_id`, and the patient behind it as the record gate finds them.

## Completeness

A summary is all-or-nothing by default, as a federated query is (§11.3,
§11.4). A member that does not answer one of the section queries, or a
member in the `native` mode whose own endpoint does not answer, fails the
summary: the answer is the status the query would take, `424` or `504`, with
an `OperationOutcome` that names each such endpoint and its §11.1 status.

Where the deployment sets `federation.best_effort = true`, send
`openEHR-federation-completeness: partial` to take the summary of the members
that answered. Every section then names the members that did not, and none
of them is `nilknown`.

A patient no member holds is a `404`, and so is a patient whose members may
not disclose a restriction, or one the demographics binding does not know:
they answer alike. A patient the identifier cross-reference could not
answer for at any member is not one of them: the summary fails with `424`,
naming each member `not-resolved`, and under `partial` every section names
each member and none of them is `nilknown`.

The header is asked before any member, and a summary whose header cannot be
written asks no member:

| The demographics binding | The answer |
|---|---|
| knows no patient for the identifier | `404` `not-found` |
| matches several patients | `422` `multiple-matches`, none of them picked |
| holds the patient with no family name, given name or text | `422` `required`: the EPS `Patient` requires a name (`ips-pat-1`) |
| is not asked about identifiers of the system you sent | `422` `not-supported` |
| does not answer within its budget | `504` `timeout` |
| fails, or its exchange cannot be audited | `502` `exception` |

## Authentication and the access log

Every request to the face passes the same client authentication as the
ITS-REST face (§13.1, N25). `metadata` takes a verified caller. A summary
runs the eleven section queries, so it takes the SMART on openEHR `aql-`
search permission on each of them: `user/aql-*.s`, or a pattern that covers
the reserved namespace, such as `user/aql-eu.ferrofed.eehrxf::*.s`. No
specification defines the scopes of this face: this is our own design until
one does. As for any query that reaches patient data, the caller must state
a purpose of use, a client must act for a professional it names at the
assurance level the issuer requires, and a national contact point's
connector must relay the attributes its issuer declares. The handler holds
the caller to the scopes the gate found covering the summary once more
before any member is asked.

The document list takes the same scopes as a summary: it names the summary
and carries the members' own documents.

A person reads their own summary through an electronic health data access
service (Regulation (EU) 2025/327 Art 3(1), Art 4). The service holds a
token confined to the person
([Patient grants](../operate/authentication.md#patient-grants)), with the
scope `patient/aql-eu.ferrofed.eehrxf::*.s`, and asks for the summary by the
person's identifier as any caller does. The gateway resolves that
identifier at the member the token's issuer is bound to and compares it
with the token's patient before the PDQm Supplier or any member is asked.
The person's own identifier gets the summary from every member that holds
them; another patient's is `403` (`patient-confinement`), an
`OperationOutcome` of type `forbidden`, with nothing sent. A professional's
request is unchanged.

Every summary that reached a member writes one access record (Annex II 3.2),
naming the verified caller, the patient, every endpoint asked and the
`ehr_id` at each, and the categories: `Patient-Summaries` by construction
(Art 14(1)(a)), beside every category the deployment's map gives the
compositions the members answered with. A document list that asked a
member's own endpoint writes one record too, naming every such endpoint
and the `ehr_id` it was asked by, and the members whose document it
carries; a list that asked no member, because none is in the `native`
mode, sends nothing to a member and writes none. The record of a person's
own summary names the person twice under Art 9: as the caller, the token's
`sub` acting in person with the purpose it states, and as the data subject,
by the identifier the service asked with and the `ehr_id` at each member.

## Errors

Every error under `{fhir-base}` is a FHIR `OperationOutcome`, served as
`application/fhir+json`. A refusal of the gateway's own, such as a
client-authentication refusal, keeps its status and its headers, and its
code and message become the issue's diagnostics:

| Status | Issue type |
|---|---|
| `400` | `invalid` |
| `401` | `login` |
| `403` | `forbidden` |
| `404` | `not-found` |
| `405`, `406`, `415`, `501` | `not-supported` |
| `408`, `504` | `timeout` |
| `424` | `incomplete` |
| `429` | `throttled` |
| `503` | `transient` |
| any other | `exception` |

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
2. It is held to the category's `Bundle` and `Composition` profiles from the
   package the deployment supplies, their `required` terminology bindings
   included ([Terminology](#terminology)). A document that breaks them is a
   `422` with one issue per finding, located by element path. A constraint
   the check cannot evaluate is listed as a `warning` in the answer, never
   passed silently.
3. Every coding that carries no `display` is given one by the terminology
   server's `$lookup`, because the openEHR side needs the text of every
   coded term. With no terminology server configured, such a coding is a
   `422` naming where it is.
4. It is mapped into one composition by the category's FHIRconnect mapping.
   The composition keeps the whole document, as it was received, in its
   `FEEDER_AUDIT.original_content`.
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
`create` on `Bundle`. Its documentation names the categories the deployment
receives and those it has no receiving member for, by their HL7 Europe
codes, and its supported profiles are the `Bundle` profiles of the
categories received. `ferrofed config check` prints the same categories,
each received one with its member
([Running it](../operate/configuration.md#running-it)).

## Prescriptions and dispensations

Electronic prescriptions and dispensations, Art 14(1)(b) and (c), are served
resource by resource: the HL7 Europe Medication Prescription and Dispense
(MPD) 1.0.0 `MedicationRequest-eu-mpd` and `MedicationDispense-eu-mpd`. The
member that holds a prescription serves it on its own `hl7-fhir-rest`
endpoint ([A member's own exchange-format endpoint](../operate/registry.md#a-members-own-exchange-format-endpoint)), and the face
passes on what that member says. The member decides whether a prescription
can still be dispensed and counts the dispensations it has left; the gateway
computes neither and holds no lock.

You declare in `[[fhir.serve]]` the members that meet that contract, per
category. A category no member is declared for is not served, and every
route of it answers `404` with an `OperationOutcome` of the issue type
`not-supported` that says so. No member is asked, and the answer is never an
empty search result, which a client could read as a patient with no
prescriptions:

| Route | Category |
|---|---|
| `GET {fhir-base}/MedicationRequest?patient.identifier=` | `Electronic-Prescriptions` |
| `GET {fhir-base}/MedicationRequest/{id}` | `Electronic-Prescriptions` |
| `POST {fhir-base}/MedicationDispense` | `Electronic-Dispensations` |

`GET {fhir-base}/metadata` lists `MedicationRequest` and
`MedicationDispense` only for a category a member is declared for, each with
its MPD profile, and declares for `MedicationRequest` the `search-type` and
`read` interactions the face serves. The receipt of a dispensation at a
declared member is planned for this release (#1042); until it ships, that
route answers `501`.

### Searching a patient's prescriptions

`GET {fhir-base}/MedicationRequest?patient.identifier=system|value` names
the patient by an identifier with its system, as `$summary` does, and takes
the `composition-` search scope over every template
(`user/composition-*.s`, for example). The search takes `patient.identifier`, `_count` and
`_format` alone; any other parameter, or a second patient, is a `400`.

The gateway resolves the patient once, then asks each declared member that
holds the patient at its own endpoint, with
`GET {own}/MedicationRequest?patient.identifier={ehr_id_system}|{ehr_id}`.
The member's own `ehr_id` is the only value about the patient the request
carries, and the identifier you sent never reaches a member. The gateway
follows each member's `next` links until its last page, and only links
under the member's own base URL. The answer is one `searchset`:

- each member's `MedicationRequest` as the member sent it: every element
  and extension kept, and every number in the text the member wrote, such
  as `1.50`; only the order of an object's members may differ, which JSON
  gives no meaning (RFC 8259 §4). Entries are in endpoint order, then in
  the member's own order. Its `id` is the one the face reads it by
  ([Reading one prescription](#reading-one-prescription)), and its
  `fullUrl` that read, `{fhir-base}/MedicationRequest/{id}`;
- one `Provenance` per prescription, whose agent is the member's
  `Organization`; the gateway adds no author and merges nothing;
- a `warning` `outcome` entry when two members hold a prescription under the
  same identifier; each member's prescription stays listed;
- `Bundle.total`, the number of prescriptions over every page, and a `self`
  link naming the parameters the search used.

With `_count=n` the gateway pages the answer itself: each page holds at most
`n` prescriptions, and its `next` link carries the gateway's own `-offset`
parameter, so a page holds no state between requests. Each page asks the
members again.

A patient that no declared member holds, and a patient whom a consent
decision hides, both get the same empty `searchset`. A member that does not
answer, answers with no `searchset`, or links a page outside its own base
fails the search under all-or-nothing: `504` for a member that is offline or
timed out, `424` otherwise, with every such member named. With
`openehr-federation-completeness: partial` and `best_effort`, the other
members' prescriptions come back and each silent member is named in a
`warning` `outcome` entry. Each search is recorded once under
`Electronic-Prescriptions`, naming every member asked.

### Reading one prescription

`GET {fhir-base}/MedicationRequest/{id}` reads one prescription by the `id`
the search gave it, and takes the `composition-` read scope over every
template (`user/composition-*.r`, for example). The `id` names the member's
own endpoint, the member's `ehr_id` for the patient and the member's own
`id` of the prescription, packed into the 64 characters a FHIR `id` allows,
so the gateway holds no state between the search and the read. A
prescription whose parts do not fit keeps the member's own `id` in the
search, and cannot be read through the face.

The `id` only locates the prescription. The gateway asks the member it
names alone, with
`GET {own}/MedicationRequest?patient.identifier={ehr_id_system}|{ehr_id}&_id={member's id}`,
so the request carries the member's own `ehr_id` and its own `id` of the
prescription, and nothing else about the patient. The member still decides
what the caller may read. A caller whose grant is confined to one patient
(`patient/`) reads that patient's prescriptions alone: an `id` naming
another patient's `ehr_id` is refused `403` and nothing is sent.

The answer is the member's `MedicationRequest` as it sent it, with its `id`
the one you read, as FHIR R4 asks of a read. The
`openEHR-federation-endpoint` and `openEHR-federation-system-id` headers
name the member that answered. The other answers:

| Answer | When |
|---|---|
| `404` `not-found` | The gateway did not write the `id`, it names no member declared for prescriptions, the gateway already places its `ehr_id` at another member, or the member holds no such prescription for that `ehr_id`. Every case gets the same text. |
| `409` `conflict` | Two members claim the `ehr_id` the `id` names, which also raises the integrity incident; or the member answered more than one prescription for one `id`, with no incident. Nothing is passed on. |
| `424` | The member answered with an error, with no `searchset`, or with another prescription. |
| `504` | The member is offline or did not answer in time. |

Each read that reached a member is recorded once under
`Electronic-Prescriptions`, naming the member and the `ehr_id` it was asked
by.

## Terminology

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
today carries the codings its FHIRconnect mappings write; applying the rules
to each mapped composition is planned
([#926](https://github.com/FerroHEALTH/FerroFED/issues/926)).

### The terminology server

A FHIR R4 terminology server is optional. When `[eehrxf.terminology]` names
one, the component asks it for three operations:

- `ConceptMap/$translate`, for a code in no preferred system. Only a match
  whose equivalence is `equivalent` or `equal` becomes a coding; a `wider`,
  `narrower`, `inexact` or other match never does.
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

### Required bindings

A `required` binding means the concept "SHALL be from the specified value
set" (FHIR R4). The receive check holds each `code`, `Coding` and
`CodeableConcept` that a `required` binding of the category's `Bundle` or
`Composition` profile covers to the expansion of its value set, read from the
package the deployment supplies. A concept outside the expansion is a `422`
finding. A `CodeableConcept` holds when one of its codings is in the
expansion.

The vendored HL7 Europe Patient Summary package carries no value set
expansion. Most value sets its `required` bindings name are those of FHIR R4
itself and of the HL7 terminology, which no package FerroFED vendors carries,
and the few the HL7 Europe and IPS packages define carry no expansion either.
Without a terminology server, every such binding is therefore listed as a
`warning` that names the value set as not evaluated. A binding inside a
slice, or on an element of a type that carries no code, is listed the same
way.

## Configuring the face

```toml
[server]
public_url = "https://gateway.example.org"   # the face names its entries under it

[fhir]
base = "/fhir"                               # {fhir-base}, never {base} or a path under {base}/v1

[fhir.operator]
name = "Example Health Network"              # the operator, an author of every summary
identifier_system = "urn:oid:2.999.9"        # optional, with identifier_value
identifier_value = "operator-1"

[[fhir.mapping]]
section = "allergies-and-intolerances"       # the slug of a section query
template = "/etc/ferrofed/mappings/allergy.opt"
files = [
  "/etc/ferrofed/mappings/allergy.yml",
  "/etc/ferrofed/mappings/allergy.context.yml",
]
context = "example_allergy.context"          # the context mapping's metadata.name

[[fhir.receive]]
category = "Patient-Summaries"               # the Art 14(1) category, by its HL7 Europe code
member = "node-a"                            # the node id of the member that stores it
package = "/etc/ferrofed/hl7.fhir.eu.eps-1.0.0-ballot.tgz"
template = "/etc/ferrofed/mappings/received-summary.opt"
files = [
  "/etc/ferrofed/mappings/received-summary.yml",
  "/etc/ferrofed/mappings/received-summary.context.yml",
]
context = "example_received_summary.context"
language = "en"                              # the composition's language, ISO 639-1
territory = "NL"                             # the composition's territory, ISO 3166-1

[[fhir.serve]]
category = "Electronic-Prescriptions"        # Electronic-Prescriptions or Electronic-Dispensations
members = ["node-a"]                         # node ids, each with its own MPD endpoint

[[fhir.serve]]
category = "Electronic-Dispensations"
members = ["node-a"]                         # each also serves Electronic-Prescriptions
```

The terminology server, when you configure one:

```toml
[eehrxf.terminology]
url = "https://terminology.example.org/fhir/"   # the server's FHIR base
timeout_ms = 2000                               # one operation's budget; 2000 unless set

[eehrxf.terminology.credentials]
bearer_token_file = "/run/secrets/terminology-token"   # or user and password
```

`client_identity`, `client_identity_file` and `trust_roots_file` give the
mutual TLS the server asks for, as for the identity services. Outside
`profile = "development"` the URL must be `https`, because the server is sent
the codes of the documents you receive.

For `[[fhir.receive]]`, set `receive_namespaces` under `[fhir]` as well,
for example `receive_namespaces = ["urn:oid:2.999.1"]`: the identifier
systems a received document's patient is resolved in.

- `[fhir]` needs a demographics binding, `[pdqm]`, because every summary
  names its patient and the members federate no demographics: configuration
  load refuses the face without one. The Supplier is asked by the identifier
  in the system `[pdqm.namespaces]` maps its namespace to, in the master
  domain, or in the system you sent when that is an absolute URI.
- `[fhir]` needs a registry and `server.public_url`, and refuses a base on a
  path the gateway serves under `{base}`, or one that overlaps the PMIR feed
  route (`pmir.path`).
- Each `[[fhir.mapping]]` is one FHIRconnect 1.0.0 context mapping, compiled
  when the configuration is read. It must map to a profile the crosswalk
  admits for an entry of its section, for example `allergyIntolerance-eu-core`
  for the allergies. A mapping that does not compile, or maps elsewhere,
  refuses the start and `ferrofed config check`.
- A section a template feeds through several profiles takes one entry per
  profile.
- The section slugs are those of the section queries: `allergies-and-intolerances`,
  `problems`, `medication-summary`, `medical-devices-and-implants`,
  `procedures`, `immunisations`, `social-history`, `pregnancy-history`,
  `advance-directives`, `observation-results` and `care-plans`.
- Each `[[fhir.receive]]` declares one category, once. Its member must be
  a node of the registry and a resolver must be configured, or the start and
  `ferrofed config check` are refused. The package is the vendored HL7
  Europe package whose profiles the category's documents are held to, and
  the mapping compiles when the configuration is read.
- Each `[[fhir.serve]]` declares one category, once, with at least one
  member, each named once. Configuration load refuses any other category and
  a member of `Electronic-Dispensations` that does not also serve
  `Electronic-Prescriptions`, because a dispensation is written only at the
  member that owns its prescription. The start and `ferrofed config check`
  are refused for a member that is no node of the registry, one with no
  `hl7-fhir-rest` endpoint of its own whose `categories` name the category,
  and a face with no resolver. With no entry, both categories are answered
  as not served ([Prescriptions and dispensations](#prescriptions-and-dispensations)).
- `[eehrxf.terminology]` needs `[fhir]`: configuration load refuses it
  alone, and refuses a URL that carries a user name or a password.
- A change to `[fhir]` or to `[eehrxf.terminology]` takes a restart.
