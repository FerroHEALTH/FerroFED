<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The patient summary over FHIR

Regulation (EU) 2025/327 Annex II 2.1 asks an EHR system for "an interface
enabling access to the personal electronic health data processed by it in
the European electronic health record exchange format". FerroFED serves that
interface as a FHIR R4 face on a base of its own, `{fhir-base}`, beside the
ITS-REST face at `{base}`. No ITS-REST path changes: `{base}` stays a
conformant ITS-REST surface (Federation Tier §4.1, N1, N28).

The face answers six requests today:

| Request | Answer |
|---|---|
| `GET {fhir-base}/metadata` | the face's `CapabilityStatement` |
| `GET {fhir-base}/Patient?identifier=system\|value` | the patient lookup: the `Patient` the identity binding holds, when a member holds the patient ([Looking a patient up](#looking-a-patient-up)) |
| `GET` or `POST {fhir-base}/Patient/$summary` | the patient summary, an HL7 Europe Patient Summary document `Bundle` |
| `POST {fhir-base}/Bundle` | a document received in the exchange format, written to the member its category is declared for |
| `GET {fhir-base}/DocumentReference` | the document list: the summary the gateway writes on request and each member's own one ([The document list](#the-document-list)) |
| `GET {fhir-base}/Binary/{token}` | a member's own document, at the URL the document list names |

The gate refuses every other path under `{fhir-base}`, and every other
method, with a `403`: a variant of a path, in another letter case, with a
percent-encoded character or a trailing or doubled slash, is refused, never
served.

## Looking a patient up

Xt-EHR D5.1 §3.4.6 `api-producer-patient` asks every producer EHR system for
"a patient lookup API sufficient for a consumer to unambiguously identify
the patient EHR in the producer's system". The face answers it with the
FHIR R4 `Patient` search by one parameter, `identifier`, the patient's
identifier with its system:

```http
GET {fhir-base}/Patient?identifier=urn:oid:2.999.1|SYNTHETIC-1
```

The gateway asks the PDQm Supplier of `[pdqm]` for the patient first, as
for a summary header, then localizes and resolves the patient as it does
any subject, through the configured identity binding (Federation Tier
§5.2). It asks no member anything, so no patient identifier reaches a node
(§5.4.1, N33). When the patient resolved at a member, the answer is a
`searchset` with one entry: a `Patient` that carries the identifier it was
looked up by and the names, birth date, gender, addresses and telecoms the
Supplier holds, each as the Supplier gives it, under a `urn:uuid:` full URL
minted for the answer. A patient with no name is still answered; only a
summary requires one. Use the identifier, not the minted id, in the requests
that follow: the face keeps no `Patient` resource to read by id.

An identifier the Supplier does not know, a patient no member holds and a
patient every member restricts answer alike, with the empty `searchset`, so
the answer never shows a restriction (Regulation (EU) 2025/327 Art 8;
§11.3). Under all-or-nothing, a member the resolver could not answer for
fails the lookup with `424`; under `partial` it is named in an `outcome`
entry beside the `Patient`.

A search by demographics, by `_id`, with a modifier, with a second
parameter beside `identifier`, or with no `identifier` is refused with `400`
and an `OperationOutcome` that names `identifier=system|value` as the
parameter the face takes. FerroFED's input is an identifier, never
demographics (§5.4.3, N32); D5.1 admits "other forms of patient identity".

`metadata` declares the lookup: the `Patient` resource with the
`search-type` interaction and the one search parameter `identifier`,
defined by `http://hl7.org/fhir/SearchParameter/Patient-identifier`, beside
the `$summary` operation.

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
- The medical alert and the functional status, which no openEHR content is
  named to feed, are always present too, each with the empty reason
  `unavailable` and a narrative saying the gateway does not supply it, so no
  reader takes their absence for "none".
- Every section has a generated narrative that says it is not exhaustive,
  names every member that gave no answer, counts what no mapping covers, and
  names, by path and member, every element a mapping could not carry into
  the section.
- When two members record a medicine with the same code, strength and
  route, the medication summary lists both entries and its narrative marks
  them as a probable duplicate, naming both members; nothing is merged.
- A composition goes to every section whose query selects it, and each
  mapping places it by its own explicit criterion: a medication use is a
  medicine or a vaccination by what its mapping reads, never by inference.
  The medication summary also takes the medication orders a member records.
- A section with no entry carries the empty reason `nilknown` only when
  every member answered and none holds content for it, and `unavailable`
  otherwise.
- Each member's data are listed as mapped, beside every other member's,
  never merged with them. A composition two members hold under one full
  version id, an imported copy (Federation Tier §10.3), is listed once: the
  copy at the member whose `system_id` is the version's `creating_system_id`,
  else the copy at the lowest endpoint id, and the section's narrative names
  the members whose copy it left out. The MyHealth@EU requirements forbid
  information present identically more than once in an exchanged document
  (Requirements Catalogue v10.0.0, 10.03).
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

The patient lookup finds whether members hold the patient's EHRs, as the
ITS-REST read `GET {base}/ehr?subject_id=` does, so it takes what that read
takes: the `composition-` read of every template, `user/composition-*.r`.
A token confined to one patient looks up that patient alone; another
patient's identifier is `403` (`patient-confinement`) before the PDQm
Supplier is asked.

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
carries. A list that lists the gateway's summary tells you that members
hold the patient's record, which is health data in itself (GDPR recital 35),
so it writes one record even when it asked no member, because none is in the
`native` mode: the record names the members the patient resolved at, each at
the endpoint it is asked through and by its `ehr_id`, as origins that
contributed nothing (`fhir/list.rs`:
`a_list_that_asked_no_member_is_recorded_once_naming_where_the_patient_resolved`).
A list of a patient no member holds lists nothing and writes no record.
A lookup that finds the patient writes one record on the same ground, naming
the members the patient resolved at, each by its `ehr_id`, with the
category `no-category` and the resource kind `EHR`, since it reached no
composition; a lookup that finds no patient writes none
(`fhir/lookup.rs`). The record of a person's
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

## Prescriptions and dispensations

The face also serves electronic prescriptions and dispensations, Art
14(1)(b) and (c), resource by resource: the search and the read of a
patient's prescriptions, and the receipt and the read of a dispensation, each
at the member that holds it ([Prescriptions and dispensations over FHIR](prescriptions.md)).

## Terminology

A code passes as it was recorded, and a coding is added beside it only where
it means the same thing; [Terminology on the FHIR face](terminology.md)
gives the rules, the terminology server and the required bindings.

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
package = "/etc/ferrofed/packages/hl7.fhir.eu.eps-1.0.0-ballot.tgz"
dependencies = [                             # the package's dependency closure, each a .tgz or a directory
  "/etc/ferrofed/packages/hl7.fhir.r4.core-4.0.1.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.base-2.0.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.base-2.0.1.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.extensions.r4-1.3.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.eu.extensions.r4-1.3.1.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.ips-1.1.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.ips-2.0.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.ipa-1.1.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.smart-app-launch-2.0.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.extensions.r4-5.2.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.extensions.r4-5.3.0.tgz",
  "/etc/ferrofed/packages/hl7.fhir.uv.xver-r5.r4-0.1.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-5.0.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-6.2.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-6.3.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-6.5.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-7.1.0.tgz",
  "/etc/ferrofed/packages/hl7.terminology.r4-7.3.0.tgz",
  "/etc/ferrofed/packages/fhir.dicom-2022.4.20221006.tgz",
  "/etc/ferrofed/packages/ihe.pharm.mpd.r4-1.0.0-comment-2.tgz",
  "/etc/ferrofed/packages/myhealth.eu.fhir.mvc-package-9.1.0.tgz",
]
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
members = ["node-a"]                         # each also declared for Electronic-Prescriptions
```

The terminology server, when you configure one:

```toml
[eehrxf.terminology]
url = "https://terminology.example.org/fhir/"   # the server's FHIR base
timeout_ms = 2000                               # one operation's budget; 2000 unless set

[eehrxf.terminology.credentials]
user = "ferrofed"
password_file = "/run/secrets/terminology-password"
```

`client_identity`, `client_identity_file` and `trust_roots_file` give the
mutual TLS the server asks for, as for the identity services. Outside
`profile = "development"` the URL must be `https`, because the server is sent
the codes of the documents you receive.

For `[[fhir.receive]]` and for `Electronic-Dispensations`, set
`receive_namespaces` under `[fhir]` as well, for example
`receive_namespaces = ["urn:oid:2.999.1"]`: the identifier systems a
received document's or dispensation's patient is resolved in. Configuration
load refuses either without it.

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
  `ferrofed config check` are refused. The package is the HL7 Europe
  package whose profiles the category's documents are held to, and
  `dependencies` lists every package its manifest and its dependencies'
  manifests name, at the exact versions they name, with the MyHealth@EU
  value sets its profiles bind: the start and `ferrofed config check`
  refuse a set that lacks one, naming it, and one that does not carry the
  category's profiles. Each is the `.tgz` the FHIR package registry serves
  (`https://packages.fhir.org/<name>/<version>`) or an unpacked directory;
  the list above is the closure of `hl7.fhir.eu.eps` 1.0.0-ballot, which
  `scripts/vendor/eehrxf.sh` and `scripts/vendor/fhir-packages.sh` fetch,
  each pinned by sha256. The mapping compiles when the configuration is
  read.
- Each `[[fhir.serve]]` declares one category, once, with at least one
  member, each named once. Configuration load refuses any other category and
  a member of `Electronic-Dispensations` that is not also declared for
  `Electronic-Prescriptions`, because a dispensation is written only at the
  member that owns its prescription. The start and `ferrofed config check`
  are refused for a member that is no node of the registry, one with no
  `hl7-fhir-rest` endpoint of its own whose `categories` name the category,
  and a face with no resolver. With no entry, both categories are answered
  as not served ([Prescriptions and dispensations over FHIR](prescriptions.md)).
- `[eehrxf.terminology]` needs `[fhir]`: configuration load refuses it
  alone, and refuses a URL that carries a user name or a password.
- A change to `[fhir]` or to `[eehrxf.terminology]` takes a restart.
