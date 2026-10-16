<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Prescriptions and dispensations over FHIR

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
| `GET {fhir-base}/MedicationDispense/{id}` | `Electronic-Dispensations` |

`GET {fhir-base}/metadata` lists `MedicationRequest` and
`MedicationDispense` only for a category a member is declared for, each with
its MPD profile, and declares for `MedicationRequest` the `search-type` and
`read` interactions the face serves, and for `MedicationDispense` the
`create` and `read` interactions. Each interaction the statement declares is one its
route serves.

## Searching a patient's prescriptions

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

## Reading one prescription

`GET {fhir-base}/MedicationRequest/{id}` reads one prescription by the `id`
the search gave it, and takes the `composition-` read scope over every
template (`user/composition-*.r`, for example). The `id` names the member's
own endpoint, the member's `ehr_id` for the patient and the member's own
`id` of the prescription, packed into the 64 characters a FHIR `id` allows,
so the gateway holds no state between the search and the read. A
prescription whose parts do not fit keeps the member's own `id` in the
search and cannot be read through the face; the search then adds a
`warning` `outcome` entry naming the member, and quoting no identifier.

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
name the member that answered. A `meta.versionId` that is a valid FHIR `id`
is answered as a weak `ETag` (`W/"3"` for version `3`), and a
`meta.lastUpdated` that is a valid FHIR `instant` as `Last-Modified`, an
HTTP-date in GMT, so you can make a conditional read. Either one absent, or
of another form, gives no such header; the gateway logs the second case by
its shape and never logs the value. The other answers:

| Answer | When |
|---|---|
| `404` `not-found` | The gateway did not write the `id`, it names no member declared for prescriptions, the gateway already places its `ehr_id` at another member, or the member holds no such prescription for that `ehr_id`. Every case gets the same text. |
| `409` `conflict` | Two members claim the `ehr_id` the `id` names, which also raises the integrity incident; or the member answered more than one prescription for one `id`, with no incident. Nothing is passed on. |
| `424` | The member answered with an error, with no `searchset`, or with another prescription. |
| `504` | The member is offline or did not answer in time. |

Each read that reached a member is recorded once under
`Electronic-Prescriptions`, naming the member and the `ehr_id` it was asked
by.

## Receiving a dispensation

`POST {fhir-base}/MedicationDispense` with `application/fhir+json` takes one
`MedicationDispense`, the FHIR R4 `create` interaction, and the
`composition-` create scope over every template (`user/composition-*.c`,
for example). The gateway writes it to the one member that owns the
prescription it names, and to no other member.

The dispensation is checked before any member is asked. A body that is no
`MedicationDispense` is a `400`; one that breaks `MedicationDispense-eu-mpd`,
or a contained `Medication` that breaks `Medication-eu-mpd`, is a `422` with
one issue per finding. Its `subject` names the patient by an identifier,
either a logical reference or a contained `Patient` that `subject`
references by `#id`, and each identifier whose system is in
`receive_namespaces` is resolved. Every one of them must name the same
`ehr_id` at every member, or the dispensation is refused `422`. Each
`authorizingPrescription` names a prescription by its business identifier,
and may carry the face's reference `MedicationRequest/{id}` from the search
beside it, which must then name the same prescription. A prescription named
by the face's reference alone is refused `422`, "name the prescription by
its identifier; the member cannot resolve the gateway's reference": the
member receives your body unchanged, so it can link the dispensation only by
the identifier it holds.

The gateway then asks every member declared for `Electronic-Prescriptions`
that holds the patient for the patient's prescriptions, as the search does,
by the member's own `ehr_id` alone, and finds the named prescriptions in the
answers itself. The prescription identifier never reaches a member. Every
member asked must answer, because a silent one could be the owner. The
dispensation is then created at the owner's own endpoint with
`POST {own}/MedicationDispense`, once, its body exactly as you sent it. The
member decides whether its prescription still admits the dispensation; the
gateway holds no lock, and it never sends a dispensation again.

An `openEHR-federation-endpoint` header is a check only: when it names an
endpoint of another member than the owner, nothing is written.

| Answer | When |
|---|---|
| `201 Created` | The owner created the dispensation. `Location` names it by the face's id, `{fhir-base}/MedicationDispense/{id}`, with the member's version when it named one, and the `OperationOutcome` names the member and every profile constraint the check did not evaluate. The face reads it at that id ([Reading a dispensation](#reading-a-dispensation)). |
| `400` | The header names another member's endpoint, or an endpoint the registry does not hold. |
| `404` `not-found` | No member holds the patient, none may say so, or no member holds the prescription. Every case gets the same text. |
| `409` `conflict` | Two members hold the prescription, which also raises the `PrescriptionIdentifierCollision` integrity incident; one member holds it twice, with no incident; or the owner refused the dispensation, named with its status. |
| `422` | The patient's identifiers disagree, the prescriptions are held by different members, a prescription is named by the face's reference alone, a reference and an identifier name different prescriptions, a reference is not to the face, or the owner is not declared for `Electronic-Dispensations`. |
| `503` | A member that could hold the prescription did not answer the lookup, or the patient could not be resolved at it. Nothing was written. |
| `504` | The owner did not answer in time. Whether it wrote the dispensation is unknown, and the gateway did not send it again: ask the member before you send it once more. |

No answer but the `201` and the `504` follows a write the member could have
made. Each dispensation that reached a member is recorded once, as a
creation under `Electronic-Dispensations` and `Electronic-Prescriptions`,
naming every member the lookup asked and the owner. Its record's place is
taken before the owner is asked, so a dispensation whose record has no place
is refused `503 access-unrecorded` and reaches no member. A withdrawal is a
dispensation sent again with `status` `entered-in-error`, routed as any
other.

Every dispensation that reaches the lookup is counted once by how it ended
in `ferrofed_fhir_dispensations_total`, labelled `outcome` alone, so a
dispensation no member owns is counted with no value
([The metrics](../operate/metrics.md#the-metrics)).

## Reading a dispensation

`GET {fhir-base}/MedicationDispense/{id}` reads a dispensation by the `id`
its receipt's `Location` named, and takes the `composition-` read scope over
every template (`user/composition-*.r`, for example). The `id` names the
member's own endpoint, the member's `ehr_id` for the patient and the
member's own `id` of the dispensation, as a prescription's `id` does. The
gateway asks that member alone, with
`GET {own}/MedicationDispense?patient.identifier={ehr_id_system}|{ehr_id}&_id={member's id}`,
and answers the member's `MedicationDispense` as it sent it, with its `id`
the one you read, its `ETag` and `Last-Modified` from the member's
`meta.versionId` and `meta.lastUpdated`, and the member named in
`openEHR-federation-endpoint`. Every other answer is the one the read of a
prescription gives ([Reading one prescription](#reading-one-prescription)),
with "dispensation" in its text: `404` alike for every `id` with no owner,
`409` for an `ehr_id` two members claim, `403` beyond a confined grant,
`424` and `504` for a member that does not answer. Each read that reached a
member is recorded once under `Electronic-Dispensations`.

The face's base, its authentication and the `[[fhir.serve]]` table are
described with the patient summary ([Configuring the face](patient-summary.md#configuring-the-face)).
