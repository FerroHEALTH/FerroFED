<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The document list over FHIR

**Audience:** integrators who list and read a patient's documents on the FHIR face. [The patient summary over FHIR](patient-summary.md) covers the face, the patient lookup and the summary itself.

## The document list

A `DocumentReference` search names every patient summary that exists for a
patient: the one the gateway writes on request, and the one each member in
the `native` mode serves itself. It is the search of the HL7 Europe EU
Health Data API Document Access Provider, with its `patient.identifier` and
`type` parameters, and the further parameters of the MyHealth@EU national
contact point API (`myhealth.eu.fhir.ncp-api` 9.1.1-ci,
`CapabilityStatementMyHealthEu`):

```http
GET {fhir-base}/DocumentReference?patient.identifier=urn:oid:2.999.1|SYNTHETIC-1&type=http://loinc.org|60591-5
Authorization: Bearer <token>
```

- `patient.identifier`, once, as `system|value`, names one patient. The
  patient is resolved as for a summary (§5.2).
- `patient` names the patient instead, by the id the lookup answered, as
  `Patient/{id}`, `{id}` or `{fhir-base}/Patient/{id}`. The gateway reads
  the identifier the store holds for the id and searches by it, so the
  answer is the one `patient.identifier` gives. An id the store does not
  hold, never recorded or past `fhir.id_lifetime`, matches nothing:
  the answer is the empty `searchset` with an `outcome` entry, `not-found`,
  asking you to repeat the lookup (FHIR R4 search §3.1.1.7), and no member
  is asked. Without `[store]` a `patient` search is a `400`, and `metadata`
  does not declare the parameter. Name the patient by one of the two, once.
- `_id` selects the entries with the ids it names. Beside the patient it
  needs no store. Alone, with one id, it names the document's patient
  through the store's record of the id, as a read does, and an id the store
  does not hold answers the empty `searchset` with the `outcome` entry
  asking you to repeat the search.
- `type`, when sent, is a token: `60591-5`, `http://loinc.org|60591-5` or
  `http://loinc.org|`. A `type` that admits no document type the list
  serves (the patient summary, and the laboratory, discharge and imaging
  documents when the list names them) matches nothing: the answer is an empty
  `searchset`, and nothing is resolved or sent.
- `category`, `contenttype`, `status`, `modality` and `bodysite`, when
  sent, are tokens in any of the four FHIR R4 token forms, and `creation`,
  `date` and `period` are dates with an optional prefix (`eq`, `ne`, `gt`,
  `lt`, `ge`, `le`, `sa`, `eb` or `ap`). `contenttype` reads
  `content.attachment.contentType` in the system `urn:ietf:bcp:13`, ignoring
  case; `modality` and `bodysite` read the R5 cross-version extensions
  `DocumentReference.modality` and `DocumentReference.bodySite` (its
  `concept`); `period` reads `context.period`. A date without an offset is
  read in UTC. The gateway evaluates each one on every entry it lists,
  its own and the ones it passes through from a member, after it has them:
  repetitions of a parameter must all match, the comma-separated values of
  one are alternatives, and an entry without the element a parameter reads
  does not match it. A value outside its grammar is a `400` naming the
  parameter and no value.
- A member that serves its own documents is asked with `category`,
  `creation`, `date`, `period` and `status` as you sent them, the
  parameters IHE MHD 4.2.2 ITI-67 defines, so less data travels; its `type`
  names the codes your search admits. The gateway still evaluates each one
  on what the member answers, so a member that ignores one changes nothing
  the list names. `contenttype`, `modality` and `bodysite` are never sent,
  since ITI-67 defines none of them.
- `_format`, when sent, is `json`, `application/json` or
  `application/fhir+json`. Any other parameter or modifier is a `400`.

The answer is a `Bundle` of type `searchset`, each entry a `DocumentReference`
that claims the `EehrxfMhdDocumentReference` profile, typed LOINC `60591-5`
in the `Patient-Summaries` category, its `subject` the identifier you sent
and the reference `Patient/{id}` to the patient's stable id. Each entry
has a stable id, the same in every list, derived like the patient's id
from its holder and the holder's id of the document (a member's own
`DocumentReference` takes it in place of the member's id), under the full
URL `{fhir-base}/DocumentReference/{id}`:

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
([A member that serves its own summary](patient-summary.md#a-member-that-serves-its-own-summary)),
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

### Laboratory reports in the list

The list also names the patient's laboratory reports, the ones the gateway
writes from the members' compositions and the ones a member serves itself
([Laboratory reports over FHIR](laboratory-reports.md)), and the discharge
reports and imaging documents a member serves itself
([Discharge reports over FHIR](discharge-reports.md),
[Imaging reports over FHIR](imaging-reports.md)), and the vaccination
history the gateway writes from each member's attested immunisation list
([The vaccination history over FHIR](vaccination-history.md)).

`metadata` declares the search: the `DocumentReference` resource with the
`search-type` interaction, its supported profile and each parameter above
with its definition, and the `Binary` read.

### Reading a listed document

With `[store]` set, `GET {fhir-base}/DocumentReference/{id}` answers the
entry a list named by `id`. The list records the patient each of its
entries stands for, sealed, as it records the patient's id, for
`fhir.id_lifetime`. The read finds that patient, runs the list for it under
the same admission, treatment relationship check and access record, and
answers the one entry with the id, `200`. An id the store does not hold,
never recorded or expired, is a `404` with an `OperationOutcome` asking you
to repeat the search, and no member is asked. An id no entry of the list
has any more is a `404`, saying so when a member did not answer. Without
`[store]` the read is a `400`, and `metadata` does not declare it.

### Reading a member's document

`GET {fhir-base}/Binary/{token}` answers a member's document as the member
sends it, with the member's media type. The token is a compact JWS the
gateway signs with ML-DSA-65 (RFC 9964) under a key it derives from
`fhir.id_key`, so every replica that shares the key verifies it, of its own
JOSE type, so the gateway takes no other token it signs for one. It is valid for
`fhir.document_url_lifetime`, 300 seconds unless set, names the
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
