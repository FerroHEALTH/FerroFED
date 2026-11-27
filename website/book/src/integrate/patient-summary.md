<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The patient summary over FHIR

Regulation (EU) 2025/327 Annex II 2.1 asks an EHR system for "an interface
enabling access to the personal electronic health data processed by it in
the European electronic health record exchange format". FerroFED serves that
interface as a FHIR R4 face on a base of its own, `{fhir-base}`, beside the
ITS-REST face at `{base}`. No ITS-REST path changes: `{base}` stays a
conformant ITS-REST surface (Federation Tier §4.1, N1, N28).

The face answers these requests for the summary and the documents, and the
prescription and dispensation requests of
[Prescriptions and dispensations over FHIR](prescriptions.md):

| Request | Answer |
|---|---|
| `GET {fhir-base}/metadata` | the face's `CapabilityStatement` |
| `GET {fhir-base}/Patient?identifier=system\|value` | the patient lookup: the `Patient` the identity binding holds, when a member holds the patient ([Looking a patient up](#looking-a-patient-up)) |
| `GET` or `POST {fhir-base}/Patient/$summary` | the patient summary, an HL7 Europe Patient Summary document `Bundle` |
| `POST {fhir-base}/Bundle` | a document received in the exchange format, written to the member its category is declared for |
| `GET {fhir-base}/DocumentReference` | the document list: the summary the gateway writes on request and each member's own one ([The document list](document-list.md#the-document-list)) |
| `GET {fhir-base}/DocumentReference/{id}` | one listed document by its stable id, with `[store]` set ([Reading a listed document](document-list.md#reading-a-listed-document)) |
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
Supplier holds, each as the Supplier gives it. A patient with no name is
still answered; only a summary requires one.

The `Patient` has a stable id, the same in every answer, under the full URL
`{fhir-base}/Patient/{id}`. The gateway derives it from the patient's
identifier alone: the HMAC-SHA-256 of `system|value` under a key derived
with HKDF-SHA-256 from `fhir.id_key`, in lowercase base32, 52 characters.
The identifier cannot be read back from the id. With `[store]` set, each
answer that names the patient also records the identifier the id stands
for in FerroFED's store, sealed with AES-256-GCM under a second key derived
from `fhir.id_key`, for `fhir.id_lifetime` (`24h` unless set) after
the last answer. A document search can then name the patient by this id
([The document list](document-list.md#the-document-list)). The derivation and the window
are FerroFED's own design; no specification governs them. The face keeps no
`Patient` resource to read by id.

`fhir.id_key`, or `fhir.id_key_file`, is 32 random bytes in base64url, for
example the output of `openssl rand 32 | basenc --base64url`. The face does
not start without it. Rotating it changes every patient id the face gives,
and the ids recorded under the old key no longer resolve, so a client
repeats the lookup.

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
([The document list](document-list.md#the-document-list)).

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
- The medication entries are selected by the template section that holds
  them, as the interoperability component places them
  (`eehrxf` `crosswalk::patient_summary::SOURCES`): the medication summary
  takes a medication order under an ad hoc section
  (`openEHR-EHR-SECTION.adhoc.v1`), and the immunisations take a medication
  action under the vaccination list section
  (`openEHR-EHR-SECTION.immunisation_list.v0`), never by the archetype
  alone, so a vaccine recorded as a medication action never reaches the
  medication summary. FerroBRIDGE's patient summary template, vendored
  under `docs/specs/ferrobridge-medication/`, records them so, and a
  `[[fhir.mapping]]` may name it as an ADL 2 `template` with the
  `archetypes` it specialises (`app/ferrofed-eehrxf/tests/it/summary/placement.rs`:
  `one_composition_holding_a_medicine_and_a_vaccine_lists_each_in_its_own_section_only`).
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

The document list, `GET {fhir-base}/DocumentReference`, and reading each
listed document are on [The document list over FHIR](document-list.md).

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
([Patient grants](../operate/patient-grants.md)), with the
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

`POST {fhir-base}/Bundle` is on
[Receiving a document over FHIR](receiving-a-document.md).

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
[gateway]
public_url = "https://gateway.example.org"   # the face names its entries under it

[fhir]
base = "/fhir"                               # {fhir-base}, never {base} or a path under {base}/v1
id_key_file = "/run/secrets/fhir-id-key"     # 32 random bytes in base64url, the key of the stable ids

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
template = "/etc/ferrofed/mappings/received-summary.opt"   # an OPT 1.4, or an ADL 2 template with archetypes
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
[eehrxf.terminology.endpoint]
url = "https://terminology.example.org/fhir/"   # the server's FHIR base
timeout = "2s"                                  # one operation's budget; 2s unless set

[eehrxf.terminology.endpoint.credentials]
user = "ferrofed"
password_file = "/run/secrets/terminology-password"
```

`[eehrxf.terminology.endpoint]` is `ferrohealth-smart`'s outbound table.
Its `tls` sub-table gives the mutual TLS the server asks for:
`client_identity` or `client_identity_file`, `trust_roots_file` and
`min_version`. Its `credentials` take HTTP Basic or a client credentials
grant under `oauth`, and its `retry` table the tries of one call, once
unless `retry.max_attempts` says more, since the face fails closed on the
first refusal. Outside
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
- `[fhir]` needs a registry and `gateway.public_url`, and refuses a base on a
  path the gateway serves under `{base}`, or one that overlaps the PMIR feed
  route (`pmir.path`).
- Each `[[fhir.mapping]]` is one FHIRconnect 1.0.0 context mapping, compiled
  when the configuration is read. It must map to a profile the crosswalk
  admits for an entry of its section, for example `allergyIntolerance-eu-core`
  for the allergies. A mapping that does not compile, or maps elsewhere,
  refuses the start and `ferrofed config check`. Its `template` is an OPT 1.4
  `.opt`, or an ADL 2 template beside `archetypes`, the ADL files it
  specialises, whose operational template the gateway builds at start.
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
  each pinned by sha256. `template` is an OPT 1.4 `.opt`, or an ADL 2
  template beside `archetypes`, every archetype it specialises or fills a
  slot with. The mapping compiles when the configuration is read.
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
