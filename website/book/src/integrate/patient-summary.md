<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The patient summary over FHIR

Regulation (EU) 2025/327 Annex II 2.1 asks an EHR system for "an interface
enabling access to the personal electronic health data processed by it in
the European electronic health record exchange format". FerroFED serves that
interface as a FHIR R4 face on a base of its own, `{fhir-base}`, beside the
ITS-REST face at `{base}`. No ITS-REST path changes: `{base}` stays a
conformant ITS-REST surface (Federation Tier §4.1, N1, N28).

The face answers two requests today:

| Request | Answer |
|---|---|
| `GET {fhir-base}/metadata` | the face's `CapabilityStatement` |
| `GET` or `POST {fhir-base}/Patient/$summary` | the patient summary, an HL7 Europe Patient Summary document `Bundle` |

The document list, a `DocumentReference` search that names the summary and
each member's own document, is planned
([#810](https://github.com/FerroHEALTH/FerroFED/issues/810)). The gate refuses every
other path under `{fhir-base}`, and every other method, with a `403`: a
variant of a path, in another letter case, with a percent-encoded character
or a trailing or doubled slash, is refused, never served.

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

1. Every section query is read as the façade reads a stored query, the
   patient bound through `$patient` and `$namespace` alone.
2. The patient is localized, checked against the consent pre-filter and
   resolved once, at every member (§5.2, §14). Each section query is then
   planned over that one resolution, so the identity services are asked once
   per summary.
3. Every section query goes to every member that holds the patient, scoped
   to that member's own `ehr_id`, through the same rewrite and outbound
   identifier-hygiene gate as any query. No patient identifier reaches a
   node (§5.4.1, N33). The queries run together under one budget (§11.5).
4. Each composition a section query selected is mapped by every FHIRconnect
   mapping the deployment supplies for that section and the composition's
   template. A composition no mapping covers is counted in the section's
   narrative, never dropped in silence.
5. The document is written: the composition, the patient, the gateway's
   `Device` and the operator's `Organization` as its authors, each member
   whose data a section holds as an author of that section, one `Provenance`
   per mapped composition naming the composition and the member, and every
   mapped resource.

The section queries run from the code that defines them, so the face works
whether or not the deployment sets `[stored_queries]`. The text is the one
the registry lists at version `1.0.0`.

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
- The `Patient` carries the identifier you asked by. Its name and birth date
  are absent, with a `data-absent-reason` of `unknown`, until the header is
  filled from the identity binding
  ([#663](https://github.com/FerroHEALTH/FerroFED/issues/663)).

The tests hold every document the face writes to a structural check against
the vendored `bundle-eu-eps`, `composition-eu-eps` and `patient-eu-eps`
profiles. Validation with HL7's FHIR Validator in CI is planned
([#688](https://github.com/FerroHEALTH/FerroFED/issues/688)).

## Completeness

A summary is all-or-nothing by default, as a federated query is (§11.3,
§11.4). A member that does not answer one of the section queries fails the
summary: the answer is the status the query would take, `424` or `504`, with
an `OperationOutcome` that names each such endpoint and its §11.1 status.

Where the deployment sets `federation.best_effort = true`, send
`openEHR-federation-completeness: partial` to take the summary of the members
that answered. Every section then names the members that did not, and none
of them is `nilknown`.

A patient no member holds is a `404`, and so is a patient whose members may
not disclose a restriction: the two answer alike.

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

Every summary that reached a member writes one access record (Annex II 3.2),
naming the verified caller, the patient, every endpoint asked and the
`ehr_id` at each, and the categories: `patient-summary` by construction
(Art 14(1)(a)), beside every category the deployment's map gives the
compositions the members answered with.

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
```

- `[fhir]` needs a registry and `server.public_url`, and refuses a base on a
  path the gateway serves under `{base}`.
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
- A change to `[fhir]` takes a restart.
