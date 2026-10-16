<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The registry in FHIR form

The specification recommends the FHIR `Endpoint` and `Organization` resources
for the registry (N19). Set `registry.format = "fhir"` and `registry.document`
names a FHIR R4 JSON `Bundle` of type `collection` or `searchset` instead,
holding only `Organization` and `Endpoint` resources: the shape an mCSD
directory delivers (§15.1). The default, `registry.format = "toml"`, is the
native form ([The registry document](registry.md#the-registry-document)).

```toml
[registry]
document = "/etc/ferrofed/registry.json"
format = "fhir"
```

The form loads into the same members as the native form, and the gateway
routes over it identically. FHIR has no place for a node or an openEHR
`system_id`, and a resource's logical id belongs to the server that holds it,
so FerroFED carries the registry's ids as identifiers in its own systems (no
specification governs these systems; they are FerroFED's design):

| FHIR element | Registry fact |
|---|---|
| `Organization.identifier` with system `https://ferrofed.eu/fhir/sid/organisation-id` | the organisation id, exactly one |
| `Organization.name` | the organisation's display name |
| `Organization.endpoint` | the endpoints whose node the organisation operates |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/endpoint-id` | the stable endpoint id used in directives (N19), exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/node-id` | the node the endpoint belongs to, exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/system-id` | that node's openEHR `system_id`, exactly one |
| `Endpoint.identifier` with system `https://ferrofed.eu/fhir/sid/creating-system-id` | each further `creating_system_id` the endpoint answers for (N21), zero or more |
| `Endpoint.connectionType` | `openehr-rest-query` in `https://ferrofed.eu/fhir/CodeSystem/connection-type`, or `hl7-fhir-rest` in `http://terminology.hl7.org/CodeSystem/endpoint-connection-type` for a member's own exchange-format endpoint |
| `Endpoint.managingOrganization` | the one managing organisation (N20) |
| `Endpoint.status` | `active`, or `suspended` for an endpoint taken out of service |
| `Endpoint.address` | the ITS-REST base URL |
| `Endpoint.extension` with url `https://ferrofed.eu/fhir/StructureDefinition/consent-refusal-code` | one `valueCode` per consent refusal code, zero or more; the `consent_refusal_codes` of the native form |

An endpoint for the openEHR Query API never carries `hl7-fhir-rest` (§15.2).
No openEHR or HL7 code for it is registered yet, so FerroFED binds the one
code N19 names, `openehr-rest-query`, in a code system of its own. The mCSD
4.0.0 `Endpoint` profile binds `connectionType` to the HL7 endpoint connection
types extensibly, so a code from another system is admitted where the value
set has none for the purpose.

References resolve inside the Bundle as FHIR R4 §2.36.4.1 resolves them: a
relative `Organization/org-a` against the root of a REST `fullUrl` such as
`https://registry.example.org/fhir/Endpoint/node-a-pub`, and an absolute
reference, a `urn:uuid:` included, against an entry's `fullUrl`. Give every
entry a `fullUrl`. Other elements (`payloadType` codings outside the category
system below, `payloadMimeType`, `period`, `header` and the rest) are not
read. A node's `product`, `version` and node identifiers have no place in
this form; a registry that needs them uses the native form.

## A member's own endpoint in FHIR form

An `Endpoint` whose `connectionType` is `hl7-fhir-rest` in HL7's
`http://terminology.hl7.org/CodeSystem/endpoint-connection-type` is
[a member's own exchange-format endpoint](registry.md#a-members-own-exchange-format-endpoint),
the native form's `hl7-fhir-rest` `[[endpoint]]`. It carries the identifiers,
the managing organisation, the status and the address of any endpoint, is
listed by the organisation that operates its node, and declares the three
keys of the native form:

| FHIR element | Native form key |
|---|---|
| `Endpoint.payloadType.coding` with system `http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs` | `categories`: one coding per category it serves, by its Art 14(1) priority category code, one or more |
| `Endpoint.extension` with url `https://ferrofed.eu/fhir/StructureDefinition/payload-profile` | `profiles`: one `valueCanonical` per profile its documents follow, one or more |
| `Endpoint.extension` with url `https://ferrofed.eu/fhir/StructureDefinition/ehr-id-system` | `ehr_id_system`: one `valueUri`, exactly one |

FHIR R4 defines `payloadType` as the type of content the endpoint serves, so
the categories are codings there, in the code system of HL7 Europe's Health
Data API that the gateway's category codes come from; a coding in another
system describes other content and is not read. FHIR R4 has no element for a
document profile (`payloadMimeType` names a serialisation) or for the
identifier system of an `ehr_id`, so those two are FerroFED's extensions (no
specification governs them; they are FerroFED's design):

```json
{
  "resourceType": "Endpoint",
  "id": "hospital-a-eps",
  "extension": [
    {"url": "https://ferrofed.eu/fhir/StructureDefinition/payload-profile",
     "valueCanonical": "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"},
    {"url": "https://ferrofed.eu/fhir/StructureDefinition/ehr-id-system",
     "valueUri": "urn:oid:2.999.4.1"}
  ],
  "identifier": [
    {"system": "https://ferrofed.eu/fhir/sid/endpoint-id", "value": "hospital-a-eps"},
    {"system": "https://ferrofed.eu/fhir/sid/node-id", "value": "hospital-a"},
    {"system": "https://ferrofed.eu/fhir/sid/system-id", "value": "cdr-a.example.org"}
  ],
  "status": "active",
  "connectionType": {
    "system": "http://terminology.hl7.org/CodeSystem/endpoint-connection-type",
    "code": "hl7-fhir-rest"
  },
  "managingOrganization": {"reference": "Organization/org-a"},
  "payloadType": [{"coding": [{
    "system": "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs",
    "code": "Patient-Summaries"
  }]}],
  "payloadMimeType": ["application/fhir+json"],
  "address": "https://cdr-a.example.org/fhir"
}
```

The endpoint meets every rule the native form's does, and `config check`
names it when it breaks one: an `hl7-fhir-rest` endpoint with no
`ehr_id_system` extension, no category coding or no profile extension, or
with a consent refusal code extension, is refused, and so is an
`openehr-rest-query` endpoint that carries a category coding or either
extension. A category coding with no `code`, a profile extension with no
`valueCanonical`, and an `ehr_id_system` extension with no `valueUri` or
given twice are refused as the FHIR form's own faults.

`config check` refuses the document with the configuration exit code, naming
the resource, when:

- an endpoint's `connectionType` carries no system (an informal string), or
  is a system and code other than `openehr-rest-query` in FerroFED's system
  and `hl7-fhir-rest` in HL7's (N19, §15.2). CP-20 is an operator point, and
  this check is how the gateway helps the operator meet it;
- an endpoint has no `managingOrganization`, or one that names no
  `Organization` of the Bundle (N20);
- an endpoint is listed by no organisation, or by two;
- an organisation or an endpoint has no id in its system or more than one, or
  an id repeats;
- the endpoints of one node disagree on its `system_id` or its operator;
- an endpoint's status is neither `active` nor `suspended`, or an organisation
  is marked inactive;
- a resource carries a `modifierExtension`, which FerroFED does not read;
- a consent refusal code extension carries no `valueCode`;
- anything the native form refuses: a duplicate `system_id`, an unusable base
  URL, or a `creating_system_id` that is a member's own.

The FHIR form and an mCSD directory keep an organisation whose endpoints left
the directory, so a refresh that deletes an organisation's last endpoint is
applied; a selector that names it then selects nothing, and the request
answers `404` with `no-destination` (§11.2).

## The definitions behind FerroFED's URLs

Every `https://ferrofed.eu/fhir/` URL of the registry in FHIR form resolves
to its FHIR R4 definition, so a receiving system can read it and a validator can check
a document against it:

| URL | Definition |
|---|---|
| `…/StructureDefinition/consent-refusal-code` | an extension of `Endpoint`, zero or more, with a `valueCode` |
| `…/StructureDefinition/payload-profile` | an extension of `Endpoint`, zero or more, with a `valueCanonical` |
| `…/StructureDefinition/ehr-id-system` | an extension of `Endpoint`, at most once, with a `valueUri` |
| `…/CodeSystem/connection-type` | a complete code system with the one code `openehr-rest-query` |
| `…/sid/organisation-id`, `…/sid/endpoint-id`, `…/sid/node-id`, `…/sid/system-id`, `…/sid/creating-system-id` | a `NamingSystem` of kind `identifier` whose preferred `uri` is the URL itself |

The access log's default origin system,
[`…/CodeSystem/access-log-origin`](access-labels.md#the-labels-a-record-carries),
and the medication extension `…/StructureDefinition/remaining-dispensations`
resolve the same way, and so does every other `https://ferrofed.eu/fhir/`
URL FerroFED writes but one family, the
[archetype-local code systems](#the-archetype-local-code-systems). FHIR R4
gives a `NamingSystem` no canonical URL, so
each identifier system's `NamingSystem` is served at the system's own URL.

The documentation site is a static site on GitHub Pages, which sets the
content type from the file extension and cannot redirect or negotiate on
`Accept`. Each definition is therefore served twice with the same bytes: at
its URL with no extension, which Pages sends as
`application/octet-stream`, and at the URL plus `.json`, which Pages sends
as `application/json; charset=utf-8` (both read from the deployed site on
2026-10-16). A FHIR client that resolves a URL gets the definition's JSON at
either; one that reads only a JSON content type fetches the `.json` form.
Neither is sent as `application/fhir+json`, the FHIR media type, because
Pages has no setting that attaches it to a file. No specification decides how a
canonical URL is hosted: this is FerroFED's own design.

## The archetype-local code systems

A FHIR document the gateway maps from openEHR writes an archetype's own
codes, the `at` codes of its `local` terminology, in a code system per
archetype: `https://ferrofed.eu/fhir/CodeSystem/archetype-local/` followed
by the archetype id, for example
`…/archetype-local/openEHR-EHR-EVALUATION.ferrofed_allergy.v1`. No openEHR,
HL7 or FHIRconnect text defines a URI for those codes, and two versions of
an archetype are two archetypes, so each archetype id names its own system.

These URLs do not resolve. The site publishes no `CodeSystem` for them,
because there is one per archetype a deployment maps, and an archetype's
codes are defined by the archetype itself. FHIR R4 makes resolution a
recommendation, not a requirement, in 4.1.2 Choosing a system: "An http:
address SHOULD resolve to some useful description of the code system." The
same section asks that "A scope of the code system URI and the correct
usage of codes and displays in its namespace SHOULD be clearly defined",
and this section is that definition: a code under
`…/archetype-local/<archetype id>` means what the archetype with that id
defines it to mean. The choice of URI is FerroFED's own design.

