| `[xcpd]` | `xcpd.audit` and `[xcpd.audit_repository]` |
| `[nl_gf]` | |
<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The registry

This page covers the registry document and its FHIR form, the registry read
from an mCSD directory, the federation id, node selection, the state the
gateway learns (resolution bindings and the `ehr_id` index), reloading the
registry, and integrity incidents.

See how it works: [follow-ups and writes](../how-it-works/follow-ups-and-writes.md), routed with what the registry holds.

## The registry document

`registry.document` names a second TOML file: the federation's members as
the operator admitted them. It declares each `[[organisation]]`, each
`[[node]]` with its openEHR `system_id`, and each `[[endpoint]]` with its base
URL, connection type and managing organisation. An unknown key, a dangling
reference or a duplicate id refuses the whole document, and so does an
organisation that manages no endpoint (§8.1: "an Organization manages 1..*
Endpoints"); a native `hl7-fhir-rest` endpoint counts as one.

The document is also the follow-up routing table (N21). A follow-up for a
version is routed on the `creating_system_id` inside its uid (§12.2). A
member's own `system_id` routes to that member without being written down.
A CDR can hold versions another system created, because an imported
composition keeps its original uid. Map every other `creating_system_id` you
know of to the endpoint that answers for it with a `[[creating_system]]`
entry:

```toml
[[creating_system]]
creating_system_id = "legacy-a.example.org"   # the middle segment of the uid
endpoint = "hospital-a"                       # an endpoint id this document declares
```

`config check` refuses, naming the `creating_system_id`, a mapping that names
an endpoint the document does not declare, a `creating_system_id` mapped
twice, and a mapping of a member's own `system_id`. Two spellings that differ
only in ASCII case are one `creating_system_id`.

An endpoint may list the ITS-REST `Error` codes its node marks a consent
refusal with. A `403` from that node whose `Error` carries one of them is
reported `consent-denied`; every other refusal is `node-error`
([Consent](consent.md)). The list is empty by default, and an empty
code refuses the document:

```toml
[[endpoint]]
id = "hospital-b"
# ... node, url, connection_type, managing_organisation
consent_refusal_codes = ["consent-refused"]   # what this node writes in Error.code
```

### A member's own exchange-format endpoint

A member CDR may serve the patient summary in the European exchange format
itself. Declare that endpoint as a second `[[endpoint]]` of the node, of
connection type `hl7-fhir-rest`, naming the categories it serves, the
profiles its documents follow, and the identifier system the gateway writes
the member's `ehr_id` in. The member is then in the `native` mode for those
categories, and in the `map` mode, the default, for every other one:

```toml
[[endpoint]]
id = "hospital-a-eps"
node = "hospital-a"
url = "https://cdr-a.example.org/fhir"
connection_type = "hl7-fhir-rest"
managing_organisation = "org-a"
categories = ["Patient-Summaries"]   # Art 14(1) priority category codes
profiles = ["http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"]
ehr_id_system = "urn:oid:2.999.4.1"  # the system the member reads its ehr_id in
```

For a patient summary, the gateway leaves the member out of its section
queries and asks this endpoint `GET {url}/Patient/$summary` with
`identifier=<ehr_id_system>|<ehr_id>` and `profile`, the member located by
its own `ehr_id` alone (§5.4.1, N33). The request carries the onward
credential, the `DPoP` proof and the signed caller identity of the node's
`openehr-rest-query` endpoint, and passes the same outbound gate. No
specification defines an identifier system for an `ehr_id` on a FHIR
interface, so the operator declares one with the member.

The endpoint is never an openEHR Query API endpoint (N19, §15.2): no query is
sent to it, `OPTIONS {base}/` does not list it (§7a.2), and a `FROM ENDPOINT`
directive or an `openEHR-federation-endpoint` header naming it is a `400`
`endpoint-unknown` (§8). A node needs an `openehr-rest-query` endpoint
beside it. The FHIR form and an mCSD directory declare one too, as
[a member's own endpoint in FHIR form](#a-members-own-endpoint-in-fhir-form)
shows.

`config check` refuses, naming the endpoint:

- an `hl7-fhir-rest` endpoint with no `ehr_id_system`, or an empty one;
- one with no category or no profile, or an empty one;
- one with `consent_refusal_codes`, which only an ITS-REST `Error` carries;
- an `openehr-rest-query` endpoint that sets `categories`, `profiles` or
  `ehr_id_system`;
- two native endpoints of one node for one category;
- a category code that is no Art 14(1) priority category
  (`Patient-Summaries`, `Electronic-Prescriptions`,
  `Electronic-Dispensations`, `Medical-Imaging`, `Laboratory-Reports`,
  `Discharge-Reports`);
- an endpoint whose profiles leave out the one the face asks its category in:
  the EPS composition, `MedicationRequest-eu-mpd` or `MedicationDispense-eu-mpd`.

A suspended native endpoint serves nothing, and the member's data for its
categories are mapped as any other member's. The `native` mode and its keys
are FerroFED's design: no specification governs them.

## The registry document in FHIR form

The specification recommends the FHIR `Endpoint` and `Organization` resources
for the registry (N19). Set `registry.format = "fhir"` and `registry.document`
names a FHIR R4 JSON `Bundle` of type `collection` or `searchset` instead,
holding only `Organization` and `Endpoint` resources: the shape an mCSD
directory delivers (§15.1). The default, `registry.format = "toml"`, is the
native form above.

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

### A member's own endpoint in FHIR form

An `Endpoint` whose `connectionType` is `hl7-fhir-rest` in HL7's
`http://terminology.hl7.org/CodeSystem/endpoint-connection-type` is
[a member's own exchange-format endpoint](#a-members-own-exchange-format-endpoint),
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

### The definitions behind FerroFED's URLs

Every `https://ferrofed.eu/fhir/` URL this page names resolves to its FHIR
R4 definition, so a receiving system can read it and a validator can check
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
resolve the same way. FHIR R4 gives a `NamingSystem` no canonical URL, so
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

## The registry read from an mCSD directory

The gateway can read its members from an IHE mCSD 4.0.0 care services
directory instead of a document, and keep them in step with it. The
specification proposes mCSD for addressing, and the registry is the local
materialisation of that mapping (§15.1, N21, Annex A.5). Set
`[registry.mcsd]` in place of `registry.document`; a configuration that sets
both is refused.

```toml
[registry.mcsd]
url = "https://directory.example.org/fhir"   # the directory's FHIR base
refresh_interval_s = 300                    # the default; 0 is refused
deadline_ms = 30000                         # one whole read or refresh, the default
max_pages = 200                             # the defaults of the three caps
max_bytes = 67108864                        # 64 MiB of answer bodies
max_entries = 50000

[registry.mcsd.credentials]                 # when the transport does not authenticate
bearer_token_file = "/run/secrets/directory-token"
```

The members are the directory's `Organization`s that carry an
`https://ferrofed.eu/fhir/sid/organisation-id` identifier and its `Endpoint`s
that carry an `https://ferrofed.eu/fhir/sid/endpoint-id` identifier, written
exactly as the FHIR form above. The rest of the directory is not the
federation's and is never read into the registry. A member organisation may
also list endpoints that are no member: endpoints of other services, such as
its XCA endpoints, one that lost the federation's identifier, or one the
directory deleted. The gateway ignores such a listing, where the document
refuses it, and logs the listing's reference, never the endpoint, once each
time it reads the directory's content. A start and a refresh over the same
content give the same registry. The URL is `http` or
`https` with no user name or password; the credentials take a bearer token or
basic credentials, each through its `_file` sibling, and never an OAuth 2.0
grant. A directory with credentials is `https` outside
`profile = "development"`, and the configuration is refused before the
directory is asked otherwise; under that profile the site is reported as
every other cleartext credential is.

At start, the gateway reads the members with ITI-90, Find Matching Care
Services: `GET [base]/Organization?identifier=…|` and
`GET [base]/Endpoint?identifier=…|`, every page. The content then passes
every check the FHIR form passes: the connection type of §15.2 (N19), one
managing organisation per endpoint (N20), unique ids, and everything the
native form refuses. A directory that cannot be read within the deadline
and the caps, or holds a registry that breaks a rule, stops the start; `config check` reads the directory the
same way and names the fault.

Every `refresh_interval_s` the gateway asks for the changes since the last
read with ITI-91, Request Care Services Updates:
`GET [base]/Organization/_history?_since=…` and the same for `Endpoint`. It
asks from 60 seconds before the `Date` the directory stamped its previous
answer with, so the directory's own clock decides and no change is missed;
a directory that sent no readable `Date` is read again whole with ITI-90. The
newest version of each resource wins, a deletion removes it, and a version
that no longer carries the federation's identifier takes it out of the
registry. A query never waits on the directory: the refresh runs on its own,
and a request that started before a refresh finishes on the registry it
started with.

A refresh that changed something goes through the same checks as a reload:

- When the changed registry passes, it replaces the running one, with the
  effects of a reload (learned routes held to it, entries for a member that
  left dropped). It logs `registry reloaded` and counts as an applied reload.
- When it breaks a rule, it is refused. The running registry stays, the
  gateway logs `registry reload refused` with the `class` of the
  [reload classes](#reloading-the-registry), and the refusal counts as a
  refused reload. A content that breaks a registry rule (an `hl7-fhir-rest`
  endpoint without the keys of a native endpoint, a `system_id` given to two
  nodes, a registry left with no node) is `registry-invalid`. A sound registry that the rest of the
  configuration no longer fits is refused under the class of that part:
  `dev-cross-reference` or `pixm` for a resolver row naming a member the
  refresh removed, `localization`, `demographic-endpoint`, `credentials`
  for a `[credentials]` section naming a removed endpoint, `node-clients`,
  `http-client`, `self-description`, `signing` or `federation`. A deleted
  endpoint is no break of its own: its organisation's listing is ignored
  and the endpoint leaves the registry. The next refresh asks again from
  the same instant, so the registry follows the directory once the
  directory, or the configuration, is put right.
- When the directory's answer runs past `deadline_ms`, `max_pages`,
  `max_bytes` or `max_entries`, the refresh is refused the same way with
  `class = "registry-budget"`; an answer that does not hold to ITI-91 is
  refused with `class = "registry-unreadable"`. Both count as a refused
  reload. Each limit bounds one whole read or refresh,
  over every page of both resource types, so a directory that links its
  pages in a cycle or answers without end cannot hold the gateway or fill
  its memory. A partial answer never becomes the registry. No
  specification governs these limits; they are FerroFED's own design.
- When the directory cannot be reached, or refuses the request with an
  HTTP error, the running registry stays and the gateway logs a warning.

`GET {base}/operator/dependencies` reports the directory as `directory`: `up`
after an answer the gateway accepted, `degraded` after an answer whose change
it refused, `failing` after an HTTP error, a malformed answer or one past a
cap, and `down` when it did not answer before the deadline or could not be
reached. A `401` or a `403` is `failing` with
`directory_fault = "refused-credentials"`: the directory refused the
credentials of `[registry.mcsd]`.
While the directory is `degraded`, `directory_fault` says which kind of
refusal holds the change back: `registry-invalid` for a content that breaks a
registry rule (the `registry-invalid` class above), and
`configuration-mismatch` for a sound registry the rest of the configuration
no longer fits (every other class). It stays `degraded`, and the gateway
serves the registry it last accepted, until a refresh is accepted: the
directory is put right, or the configuration is changed and reloaded. The
body names the class only; the log line `registry reload refused` carries
the precise `class`. The directory's state never gates readiness.

A `SIGHUP` reload with a directory rebuilds the federation over the registry
the directory gave, applying `[credentials]`, `[dev]`, `[pixm]`, `[pdqm]`,
`[xcpd]` and `[nl_gf]`; it never asks the directory. A change to
`[registry.mcsd]` takes a restart, and a change between a document and a directory is refused as
`registry-presence`.

A directory change goes through the same checks, so it is refused when the
localizer cannot place the new members. Under `[nl_gf.nvi]`, or `[xcpd]`, a
member the directory adds without a custodian URA (or a community) mapped to
it makes the change refused as `localization`: the gateway keeps the previous
registry, logs the refusal and counts it in `ferrofed_registry_reloads_total`
([Metrics](metrics.md)), and asks again from the same instant at the next
refresh. `GET {base}/operator/dependencies` shows the directory `degraded` with
`directory_fault = "configuration-mismatch"` until the change is accepted. Map
the member first, then publish it.

The registry keeps each organisation's identifiers as the directory
publishes them, beside the `organisation-id` one: every identifier with a
system and a value, ordered and each once. They name organisations, never
patients, and a registry document in the native form carries none. Under
`[nl_gf.nvi]` the URA among them (`http://fhir.nl/fhir/NamingSystem/ura`,
Annex B §B.2) gives the localizer its custodian map, so a Dutch directory
needs no `custodians` table: a member organisation that publishes its URA
before its endpoint is never unmapped, and a refresh that adds it applies
([Dutch localization](localization.md#dutch-localization-nl_gfnvi)).

## Federation id

A gateway that federates names its federation, and refuses to boot without
the name:

```toml
[federation]
id = "rso-example"
```

The id is `federation.id` of the `OPTIONS {base}/` self-description (§7a.2,
N30). It has no default, because it is the deployment's to choose, and an
empty id is refused. It is named in the startup log line.

## Node selection

A gateway that federates (`registry.document` or `[registry.mcsd]` is set)
declares how an undirected patient query finds its nodes, and refuses to
boot without the declaration:

```toml
[federation]
node_selection = "ask-all"
```

`ask-all` is the selection for a deployment with no localization service (the
specification's reference flow, Variant B; N4). Every active member is a
candidate: the gateway asks every member's cross-reference where the patient
is, dispatches the query only to the members that return an `ehr_id`, and
reports the others as `not-resolved` without failing the query. The selection
is named in the startup log line.

`localized` derives the node set from a localizer (N4, N10, §14.1):

```toml
[federation]
node_selection = "localized"

[federation.localization]
on_failure = "closed"   # the default; "ask-all" widens on failure
timeout_ms = 5000       # the default; below overall_timeout_ms, 0 is refused
```

For an undirected patient query, the gateway first asks the localizer which
members might hold the patient's data. A member it does not name is reported
`not-localized` and is never asked, and `complete` stays `true`, because that
member was never in scope. The cross-reference then resolves the patient at
the named members only. A directed query (the `FROM ENDPOINT` directive or the
targeting headers) is never localized: the directive selects its node set
(§8). A query that names no patient is refused with a `400` under this
selection, because localization is keyed on the patient and no node set is
defined (N4). The read of an EHR by subject, `GET {base}/v1/ehr?subject_id=…`,
names the patient and no node, so it is localized as an undirected query is:
the specification does not limit N4 to AQL, and a deployment that relies on
its localizer to narrow which members learn of a patient keeps that narrowing
on every patient route
([Reading an EHR by subject](../integrate/follow-ups.md#reading-an-ehr-by-subject)).

When the localizer does not answer within `timeout_ms`, or fails, the gateway
fails closed: it asks no member, reports every member `not-localized` with the
localizer's error, and carries the same error as
`meta.federation.localization.error`, so an outage never reads as a patient
with no data (§14.1). The status stays `200` and `complete` stays `true`,
since no member in scope failed. `on_failure = "ask-all"` asks every member
instead, still with the error in `meta.federation`; it holds only where it is
written, and `OPTIONS {base}/` declares the policy either way. A localizer
that answers that no member holds the patient's data leaves every member
`not-localized` with no error.

The localizer is the IHE XCPD binding when `[xcpd]` is set
([XCPD localization](localization.md#xcpd-localization-xcpd)), and the NVI of
the Dutch Generic Functions when `[nl_gf.nvi]` is
([Dutch localization](localization.md#dutch-localization-nl_gfnvi)); setting
both refuses to boot. Otherwise it is
the resolver: the PIXm resolver when `[pixm]` is set, which names the members
whose domain holds the patient at the PIX Manager
([`[pixm]` as the localizer](identity.md#pixm-as-the-localizer)), or the
[development cross-reference](identity.md), under `profile = "development"`,
which names the members its `[dev]` rows map the patient at. The localized
selection with no localizer refuses to boot, and so do
`[federation.localization]`, `[xcpd]` and `[nl_gf.nvi]` under `ask-all`.

## Resolution bindings

The specification lets a query's resolution leave a binding behind for the
client session: which member holds which `ehr_id`, so a follow-up on a path
`ehr_id` reaches the right node (§12.5.1 step 2). The session is the caller
the gateway verified ([Client authentication](authentication.md)): the
token's issuer, subject and client together. A federated query and a read of
an EHR by subject record the `{node, ehr_id}` pairs their resolution found
under that caller, and a routed request by the same caller is routed by them
before the `ehr_id` index. Another caller never sees them. A consent denial
of a member drops every binding of that caller naming it (N27a); a binding
records where an `ehr_id` was resolved, never a consent decision. Bindings
live in memory only, keyed by `ehr_id` and never by a patient identifier,
and a restart forgets them.

Both bounds are settings, checked at boot:

```toml
[federation]
binding_ttl_ms = 900000    # 15 minutes after the last resolution that returned the binding, the default; 0 is refused
binding_capacity = 100000  # ehr_id bindings over every caller, the default; 0 is refused
```

Each binding has a lifetime of its own. A resolution that returns a
`{node, ehr_id}` pair holds it, or renews it, for `binding_ttl_ms`; a binding
the caller's later resolutions no longer return expires `binding_ttl_ms`
after the last one that did, however many other resolutions the caller makes
meanwhile. One exception holds for as long as the caller has a live binding:
an `ehr_id` the caller's resolutions placed at two members or more stays a
collision, and a follow-up on it is refused `409` (`ehr-id-collision`),
even after one claimant's binding lapses (§12.5.2, N42). A caller's bindings
can span several patients, so the gateway never lets a lapse narrow a
collision to the member where one patient resolved. Only an identity change,
a consent denial of a claimant, a claimant leaving the registry, or your
[clear](#clearing-a-held-collision) clears it.

When the bindings are full, a new one first drops every binding that has
expired, then the bindings of the other callers whose last binding expires
soonest, whole; a binding that still does not fit is not kept, which costs
that caller's follow-up the next step of §12.5.1, never a wrong node.

The lifetime is a correctness bound. An identity merge or split at the
identity source can make a binding stale, and a resolution after the change
no longer returns it, so a stale binding is routed on for at most
`binding_ttl_ms` after the last resolution that returned it. Set it no longer
than you would accept a follow-up being routed on a superseded identity. With `[pmir]`, a merge the
identity source reports drops the bindings it could have made stale as it
happens, on the replica that receives it
([The identity feed](identity.md#the-identity-feed-pmir),
[Several replicas](identity.md#several-replicas)); the
specification marks this lifecycle track provisional.

## The `ehr_id` index

The gateway also keeps an index of which member holds which `ehr_id`, shared
by every client. It learns an entry when a resolution finds the patient's
`ehr_id` at a member, and when a member answers a request under that `ehr_id`
with a success. A follow-up on a path `ehr_id` that names no node and has no
binding is routed by the index before the gateway falls back to asking every
member (§12.5.1). The index holds `ehr_id`s and member ids only, lives in
memory, and forgets the least recently used `ehr_id` once it is full:

```toml
[federation]
ehr_index_capacity = 100000   # ehr_ids held, the default; 0 is refused
```

A forgotten or never-learned entry costs a later request one fallback step,
never a wrong route: a read then asks every member, and a write is refused
until the client names its node. An `ehr_id` seen at two members is held at
both, the index raises the index-insert alarm of §12b.2 once (an
`IndexInsertCollision` incident, see [Integrity incidents](#integrity-incidents)),
and from then on it routes neither: a request for that `ehr_id` that names
no node is refused `409` (`ehr-id-collision`). A held collision has no expiry
of its own, because nothing the gateway observes shows that a node was
remedied. It lasts until you clear it
([Clearing a held collision](#clearing-a-held-collision)), the entry is
forgotten as least recently used, or the gateway restarts; after that, a
read probes every member again, and a collision that still stands is found
and reported again.

## Reloading the registry

Send `SIGHUP` to a running `ferrofed serve` to apply a changed registry
document without a restart:

```text
kill -HUP <pid of ferrofed>
docker kill --signal HUP <container>
```

The gateway reads the configuration again from where it read it at start:
the `--config` file, or the file `FERROFED_CONFIG` names, with the process's
`FERROFED__` environment over it. It checks the result exactly as `serve` and
`config check` do at start, secrets and `_file` siblings included. The
gateway reloads on the signal only and never watches the file, so write the
new document completely, then send the signal.

Nine sections take effect on a reload:

| Reloaded | Needs a restart |
|---|---|
| `[registry]`: the document's contents, its path and its `format` | `[registry.mcsd]`, the directory the registry is read from |
| `[credentials]`, each endpoint's onward TLS included | `[server]`, `public_url`, `trusted_proxies`, `forwarded_header` and `replica` included, and `[signing]`, `rotation_overlap_s` included |
| `[dev]` | `[telemetry]` and `[metrics]` |
| `[pixm]` | `[federation]`, `federation.demographic_endpoint` included, and `[stored_queries]` |
| `[pdqm]` | `[pmir]` |
| `[xcpd]` | `xcpd.audit` and `[xcpd.audit_repository]` |
| `[nl_gf]` | `[audit]` and `[config]` |
| `[access_log]` | `[fhir]` and `[eehrxf]` |
| `[auth]`: the issuers, how each is verified, its key set read again | `auth.audience`, and a changed `[server.tls]` or `[metrics.tls]` path |

The same reload reads the certificate files of `[server.tls]` and
`[metrics.tls]` again ([TLS on the listeners](configuration.md#tls-on-the-listeners)).
A request admitted before the reload finishes as admitted, and every request
that arrives after it is verified against the new `[auth]`, on the client
face and the admin listener alike. Every file and check is read before
anything changes, so a reload whose `[auth]`, certificate files or registry
fails is refused as a whole.

`federation.demographic_endpoint` keeps its running value until a restart,
and the document must still declare it: a reload whose document drops that
endpoint is refused (`demographic-endpoint`, below).

A reload whose file changes `profile` is refused (`profile`, below), so
everything the development profile admits, the development cross-reference
and consent table, and a credential or patient identifier over plain `http`
([What must travel encrypted](configuration.md#what-must-travel-encrypted)),
follows the profile the process started with.

A valid configuration replaces the running registry at once. A request that
started before the reload finishes on the registry it started with, nodes
and credentials included; every request that starts after it uses the new
one. An added endpoint gets its node client and its credentials, and a
removed endpoint is never called again. What the gateway has learned stays,
held to the new document:

- a learned `creating_system_id` route the new document maps to another node
  is withdrawn and raises a `RegisteredCreatingSystemConflict` incident (see
  [Integrity incidents](#integrity-incidents)), and stays withdrawn;
- every `ehr_id` index entry and resolution binding that names a member the
  document no longer holds is dropped. An entry that names such a member
  beside others is dropped whole, so a collision is never narrowed to the
  member that remains; a later read asks every member again. An entry a
  request already running learns after the reload, naming a member that
  left, is dropped the first time a request looks it up, with the same
  effect: a read asks every member, and a write without a target header is
  refused `400` (`target-required`).

The reload logs `registry reloaded` at `INFO` with `members` (how many the
registry now holds), `endpoints_added`, `endpoints_removed`,
`members_removed`, `incidents`, `index_dropped` and `bindings_dropped`. A
changed setting outside the reloaded sections is logged at `WARN` under
`settings`, by key (`server.listen`, `federation.binding_ttl_ms`), and keeps
its running value until a restart; the rest of the reload applies.

A configuration that does not load is refused, and the running registry
stays. The gateway logs `registry reload refused` at `ERROR` with the failure
`class`, the `config` file and the registry `document`, and never a value of
either file, a credential or a header. Run `ferrofed config check` against
the same file to see the fault. The classes are:

| `class` | The fault |
|---|---|
| `configuration` | the configuration file does not read or resolve |
| `registry-unreadable` | the registry document, or the directory, cannot be read |
| `registry-invalid` | the registry document, or the directory's content, breaks a registry rule |
| `registry-directory` | the directory of `[registry.mcsd]` cannot be asked as configured |
| `registry-budget` | a refresh of the directory ran past its deadline or a cap |
| `credentials` | a `[credentials]` section names an endpoint the document does not declare |
| `demographic-endpoint` | `federation.demographic_endpoint` names an endpoint the new document does not declare |
| `dev-cross-reference`, `pixm`, `resolvers` | the resolver refuses the new members, or both resolvers are set |
| `localization` | the localizer refuses the new members, or the node selection has none |
| `node-clients`, `http-client`, `self-description` | the node clients or the `OPTIONS {base}/` body cannot be built |
| `federation` | a registry is configured without `federation.node_selection` or `federation.id` |
| `signing` | a registry is configured without `[signing]` |
| `registry-presence` | `registry.document` or `[registry.mcsd]` was set, unset or swapped for the other, which takes a restart |
| `profile` | `profile` was changed, which takes a restart |
| `cleartext` | a credential or a patient identifier would travel over a URL that is not `https`, outside the development profile the process started with |
| `auth` | the gateway federates and `[auth]` trusts no issuer |
| `listener-certificate` | the certificate files of the listener named in `table` do not form a pair |

Where no signal reaches the process, as in a container with no shell,
`POST /admin/reload` on the admin listener runs the same reload
([Reloading over the admin listener](metrics.md#reloading-over-the-admin-listener)).
Each reload is counted by `result` on the [metrics](metrics.md) surface, and
the log lines record what it changed.

## Integrity incidents

A federation integrity defect is reported to you, the federation operator, as
an incident: one `ERROR` line under the log target `ferrofed::integrity`,
written once when the gateway detects the defect (§12.5.2, §12b.2, N42). The
line carries a stable `kind`, the routing ids involved and a message, and
never a request body, a header value or a patient identifier. These kinds
reach the log today:

| `kind` | When | Fields |
|---|---|---|
| `EhrIdCollision` | A request addressed an `ehr_id` that two members or more claim, and was refused `409` (`ehr-id-collision`). One line per refused request. | `ehr_id`, `detection` (`binding`, `index` or `ask-all`: the routing step that found the claimants), `claimants` (their endpoint ids) |
| `IndexInsertCollision` | The `ehr_id` index learned an `ehr_id` it already held at another member: the index-insert alarm of §12b.2. One line when the second claimant is learned, and one more for each further claimant. | `ehr_id`, `claimants` (the member node ids) |
| `LearnedCreatingSystemConflict` | A `creating_system_id` the registry document does not map was seen at two nodes, so the route learned for it is withdrawn and neither node is routed on (§12.2, N21). One line when the route is withdrawn. | `creating_system_id`, `first_endpoint_id`, `second_endpoint_id` |
| `RegisteredCreatingSystemConflict` | A route learned for a `creating_system_id` names another node than the registry document maps it to, seen in an answer or found when the registry is reloaded (see [Reloading the registry](#reloading-the-registry)). The learned route is withdrawn and the document's mapping is used. One line when the route is withdrawn. | `creating_system_id`, `node_id` (the node the document maps it to), `endpoint_id` (the endpoint the learned route named) |

The `ehr_id` is node-local and names no patient (§5.2), so the line names it
when it is a bare UUID. Any other form could be a patient identifier a client
wrote in a path, so the line then leaves the `ehr_id` field out.

Two nodes holding one `ehr_id` breaks the identifier-integrity conditions of
§12b.2, which admission should have checked, so the remedy is at the node.
The `claimants` name the members that hold the `ehr_id`. Have the node that
issued or adopted it in error fix it, then clear the collision the gateway
holds ([Clearing a held collision](#clearing-a-held-collision)); a restart
forgets it too, and the index forgets it when the entry is the least
recently used one past the index capacity. Until then, requests that name
no node are refused, and a client can still reach one of the members by
naming its endpoint in the `openEHR-federation-endpoint` header.

### Clearing a held collision

Nothing the gateway observes shows that a node remedied a collision, and
what a federation does about one is left to its governance (§12b.2, §18),
so the clear is yours. Once the node at fault has fixed it, send the
`ehr_id` to the [admin listener](metrics.md#who-the-admin-listener-serves)
with an access token that carries the `operator_scope` of its issuer:

```http
POST /admin/ehr-ids/7d44b88c-4199-4bad-97dc-d78268e01398/clear
Authorization: Bearer <an access token with the operator scope>
```

The operator console sends the same request from its integrity view, to one
replica or to each in turn, when its `[admin]` table names the replicas'
admin listeners ([The operator console](operator-console.md)).

- The gateway forgets that `ehr_id` from the `ehr_id` index and from the
  resolution bindings of every caller. The request takes the node-local
  `ehr_id` from the `claimants` incident and nothing else: no body and no
  patient identifier.
- The next read of the `ehr_id` that names no node asks every member
  again. One member holding it is routed to; two or more are refused `409`
  (`ehr-id-collision`) again, with a new `EhrIdCollision` incident
  (`detection` `ask-all`). A write is never probed for, so one that names no
  node answers `400` (`target-required`) until a read or a resolution
  teaches the gateway its owner (§12.5.1, N41).
- It answers `200` with what this process held, also when it held nothing:

  ```json
  {
    "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
    "held": "collision",
    "claimants": ["node-a", "node-b"],
    "indexed": true,
    "sessions": 1,
    "scope": "replica"
  }
  ```

  `held` is `collision` (two members or more), `owner` (one) or `nothing`;
  `claimants` are member node ids; `indexed` says whether the index held an
  entry; `sessions` counts the callers whose binding was dropped. `ehr_id`
  is `null` for an `ehr_id` that is no bare UUID.
- It is refused `400` for a path value that is no `HIER_OBJECT_ID`
  (`ehr-id-invalid`) or a body (`body-invalid`), `401` and `403` as every
  admin write action is, and `501` when the gateway runs without a
  registry. Where `[metrics] listen` is unset, the action does not exist.
- `scope` is `replica`: the index and the bindings are per process, so with
  several replicas send the clear to each replica's admin listener
  ([Running several replicas](deployment-shape.md#running-several-replicas)).
- The clear is recorded with the operator who ran it, as every admin write
  action is ([Who the admin listener serves](metrics.md#who-the-admin-listener-serves)),
  logged at `INFO` under `ferrofed::integrity`, and a cleared collision is
  listed under `cleared` in `GET {base}/operator/incidents` beside the
  incidents, which stay as they were reported.

The gateway counts every incident by `kind` on its [metrics](metrics.md)
surface, `ferrofed_integrity_incidents_total`, and the line carries the
routing ids you act on. The request line of a refused request carries
its `409` and its `request_id`; the incident line does not name the request.
