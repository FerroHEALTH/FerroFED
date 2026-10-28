<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Prescriptions and dispensations over FHIR

Electronic prescriptions and electronic dispensations are two of the
priority categories of Regulation (EU) 2025/327, Art 14(1)(b) and (c)
(vendored as
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml)).
The FHIR face serves them resource by resource in the profiles of the
[HL7 Europe Medication Prescription and Dispense (MPD)](http://hl7.eu/fhir/mpd)
guide 1.0.0, `MedicationRequest-eu-mpd` and `MedicationDispense-eu-mpd`
(vendored as
[`docs/specs/eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz)).

The member CDR that created a prescription serves it on its own
`hl7-fhir-rest` endpoint
([A member's own exchange-format endpoint](../operate/registry.md#a-members-own-exchange-format-endpoint)),
and the face passes on what that member says. The member decides whether a
prescription can still be dispensed and counts the dispensations it has
left. The gateway computes neither, holds no lock and stores no
prescription.

Who reads which part:

- **A client** (a pharmacy or prescribing system, or a national contact
  point): the routes from [Searching a patient's prescriptions](#searching-a-patients-prescriptions)
  to [Every answer](#every-answer), and [The locator ids](#the-locator-ids).
- **An operator**: [Declaring the serving members](#declaring-the-serving-members),
  [Access records and the counter](#access-records-and-the-counter) and
  [The capability statement](#the-capability-statement).
- **A member CDR's operator**: [The member contract](#the-member-contract)
  and [Remaining dispensations](#remaining-dispensations).

Each rule below names the test that holds it where one exists. The tests
are under `app/ferrofed-server/tests/it/` unless another path is given.

## Declaring the serving members

You declare in `[[fhir.serve]]` the members that meet
[the member contract](#the-member-contract), one entry per category:

```toml
[fhir]
base = "/fhir"
receive_namespaces = ["urn:oid:2.999.1"]   # needed once dispensations are served

[[fhir.serve]]
category = "Electronic-Prescriptions"
members = ["node-a", "node-b"]             # node ids, each with its own MPD endpoint

[[fhir.serve]]
category = "Electronic-Dispensations"
members = ["node-a", "node-b"]             # each also declared for Electronic-Prescriptions
```

`receive_namespaces` names the identifier systems a dispensation's patient
is resolved in. Configuration load refuses `Electronic-Dispensations`
without it. Each member also needs an `[[endpoint]]` of connection type
`hl7-fhir-rest` whose `categories` name the category, whose `profiles`
carry its MPD profile, and whose `ehr_id_system` names the system the
member reads its `ehr_id` in.

The start and `ferrofed config check` are refused, naming the entry, for:

- a category other than `Electronic-Prescriptions` and
  `Electronic-Dispensations`, a category declared twice, an entry with no
  member, and a member named twice;
- a member of `Electronic-Dispensations` that is not also declared for
  `Electronic-Prescriptions`, because a dispensation is written only at the
  member that owns its prescription
  (`fhir/serving.rs`:
  `a_dispensation_member_that_serves_no_prescription_is_refused`);
- a member the registry does not hold, one with no endpoint of its own
  serving the category, an endpoint whose profiles leave out the MPD
  profile, and a face with no resolver (`fhir/serving.rs`:
  `a_member_the_registry_does_not_hold_refuses_the_start`,
  `a_member_with_no_endpoint_of_its_own_refuses_the_start`,
  `a_member_whose_endpoint_serves_another_category_refuses_the_start`,
  `an_endpoint_without_the_mpd_profile_refuses_the_start`).

`config check` prints one `note:` line per declared category, naming its
members, and one per category declared for no member
([Running it](../operate/configuration.md#running-it)). A change to
`[fhir]` takes a restart. The rest of `[fhir]`, its base and its
authentication are described with the patient summary
([Configuring the face](patient-summary.md#configuring-the-face)).

A category no member is declared for is not served. Every route of it
answers `404` with an `OperationOutcome` of the issue type `not-supported`
that names the category, and no member is asked. The answer is never an
empty search result, which a client could read as a patient with no
prescriptions (`fhir/serving.rs`:
`with_no_member_declared_every_route_answers_not_served_and_asks_no_member`,
`a_served_category_is_never_answered_as_not_served`):

| Route | Category |
|---|---|
| `GET {fhir-base}/MedicationRequest?patient.identifier=` | `Electronic-Prescriptions` |
| `GET {fhir-base}/MedicationRequest/{id}` | `Electronic-Prescriptions` |
| `POST {fhir-base}/MedicationDispense` | `Electronic-Dispensations` |
| `GET {fhir-base}/MedicationDispense/{id}` | `Electronic-Dispensations` |
| `GET {fhir-base}/MedicationRequest/{id}/_history/{vid}` | `Electronic-Prescriptions` |
| `GET {fhir-base}/MedicationDispense/{id}/_history/{vid}` | `Electronic-Dispensations` |

## The member contract

FerroFED relies on each declared member for what the gateway does not
compute. The [instructions for use](../operate/instructions-for-use.md#configuration-for-a-deployment-in-the-eu)
ask the operator to confirm this with each member's operator at admission.
A member that serves `Electronic-Prescriptions`:

- is the member that created each prescription it serves, never one
  holding an imported copy;
- holds the prescriber's signature, and states each prescription's
  dispensing status and the dispensations it has left
  ([Remaining dispensations](#remaining-dispensations));
- answers `GET {own}/MedicationRequest?patient.identifier={ehr_id_system}|{ehr_id}`
  with a `searchset` of `MedicationRequest-eu-mpd` resources, each with a
  logical `id`, and with `next` links under its own base URL;
- answers the same search with `&_id={its id}` with that one prescription.

A member that also serves `Electronic-Dispensations`:

- creates a dispensation on `POST {own}/MedicationDispense` and answers
  `201` with a `Location` of the created resource, or with the resource
  and its `id` in the body;
- answers `GET {own}/MedicationDispense?patient.identifier={ehr_id_system}|{ehr_id}&_id={its id}`
  with that one dispensation;
- refuses, with a `4xx` status other than `401`, a dispensation its own
  state no longer allows, such as one beyond the remaining dispensations;
- refuses a second dispensation carrying a dispense identifier
  (`MedicationDispense.identifier`) it already holds, so a pharmacy that
  sends a dispensation again after a time-out cannot have it written twice.

The member still decides what each caller may read and write: every
request carries the onward credential, the `DPoP` proof and the signed
caller identity of the member's `openehr-rest-query` endpoint, and passes
the same outbound gate as an ITS-REST request
([A member's own exchange-format endpoint](../operate/registry.md#a-members-own-exchange-format-endpoint)).
The member's `ehr_id` and its own `id` of a resource are the only values
about the patient a request carries; no patient identifier a client sent
reaches a member (§5.4.1, N33).

Concurrent dispensations of one prescription at two pharmacies are the
member's to serialise. The gateway refuses a dispensation the owner already
holds under the same business identifier, so a dispensation sent again is
never written twice ([Receiving a dispensation](#receiving-a-dispensation),
hazard I-12 of the
[clinical safety risk file](../evaluate/clinical-safety.md#the-interoperability-component)).

## Searching a patient's prescriptions

`GET {fhir-base}/MedicationRequest?patient.identifier=system|value` names
the patient by an identifier with its system, as `$summary` does, and takes
the `composition-` search scope over every template
(`user/composition-*.s`, for example). The search takes `patient.identifier`,
`_count` and `_format` alone, and `-offset` on a page the gateway linked;
any other parameter, or a second patient, is a `400`
(`fhir/prescriptions/restricted.rs`:
`a_search_naming_no_patient_or_another_parameter_is_refused_and_nothing_is_sent`).

The gateway resolves the patient once, then asks each declared member that
holds the patient at its own endpoint, with
`GET {own}/MedicationRequest?patient.identifier={ehr_id_system}|{ehr_id}`,
and asks no member that does not hold the patient
(`fhir/prescriptions/mod.rs`: `each_member_is_asked_by_its_ehr_id_alone`,
`a_member_that_does_not_hold_the_patient_is_not_asked`). It follows each
member's `next` links until its last page, at most 64 pages a member, and
only links under the member's own base URL (`fhir/prescriptions/paging.rs`:
`a_members_next_page_is_followed_under_its_own_base`,
`a_next_link_outside_the_members_base_fails_the_search_and_is_not_followed`).
The answer is one `searchset`:

- each member's `MedicationRequest` as the member sent it: every element
  and extension kept, and every number in the text the member wrote, such
  as `1.50`; only the order of an object's members may differ, which JSON
  gives no meaning (RFC 8259 §4). Entries are in endpoint order, then in
  the member's own order. Its `id` is the [locator id](#the-locator-ids),
  and its `fullUrl` the read, `{fhir-base}/MedicationRequest/{id}`;
- one `Provenance` per prescription, whose agent is the member's
  `Organization`; the gateway adds no author, signs nothing and merges
  nothing (`fhir/prescriptions/mod.rs`:
  `each_members_prescriptions_are_answered_as_sent_with_a_provenance_naming_it`);
- a `warning` `outcome` entry when two members hold a prescription under the
  same identifier; each member's prescription stays listed
  (`a_prescription_identifier_two_members_hold_is_listed_once_per_member_and_warned`);
- `Bundle.total`, the number of prescriptions over every page, and a `self`
  link naming the parameters the search used.

With `_count=n` the gateway pages the answer itself: each page holds at most
`n` prescriptions, and its `next` link carries the gateway's own `-offset`
parameter, so a page holds no state between requests. Each page asks the
members again (`fhir/prescriptions/paging.rs`:
`the_gateway_pages_its_answer_by_count`).

A patient that no declared member holds, and a patient whom a consent
decision hides, both get the same empty `searchset`
(`fhir/prescriptions/restricted.rs`:
`an_unknown_patient_and_a_restricted_one_answer_alike_and_nothing_is_sent`).
A member that does not answer is handled as [Completeness](#completeness)
says.

## Reading one prescription

`GET {fhir-base}/MedicationRequest/{id}` reads one prescription by the
[locator id](#the-locator-ids) the search gave it, and takes the
`composition-` read scope over every template (`user/composition-*.r`, for
example). The gateway asks the member the id names alone, with
`GET {own}/MedicationRequest?patient.identifier={ehr_id_system}|{ehr_id}&_id={member's id}`
(`fhir/prescriptions/read.rs`:
`the_member_is_asked_by_its_ehr_id_and_its_own_id_alone`). A caller whose
grant is confined to one patient (`patient/`) reads that patient's
prescriptions alone: an id naming another patient's `ehr_id` is refused
`403` and nothing is sent
(`a_caller_confined_to_one_patient_reads_that_patients_prescription_alone`).

Where a consent pre-filter is configured, the read is held to it as the
search is. The read names no patient, so the gateway asks the identity
binding for the patient behind the id's `ehr_id`, in the namespaces
`[access_log] patient_namespaces` names, and asks the pre-filter about the
member before anything is sent. A member it denies is asked nothing, and the
read answers as an id the face never wrote, with no trace of the
restriction (Regulation (EU) 2025/327 Art 8, Art 11(5);
`fhir/prescriptions/restricted.rs`:
`a_read_at_a_member_the_prefilter_denies_answers_as_an_unknown_id_and_asks_nothing`).
A denial set aside for an emergency purpose is asked and named in the access
record. When the binding cannot name the patient, or no namespace is
configured, the pre-filter has nothing to decide and the member stays the
gate, as when the pre-filter gives no signal. The same holds for a
dispensation read and a version read.

The answer is the member's `MedicationRequest` as it sent it, with its `id`
the one you read, as FHIR R4 asks of a read
(`a_prescription_the_search_lists_is_read_at_its_member_as_sent`). The
`openEHR-federation-endpoint` and `openEHR-federation-system-id` headers
name the member that answered. A `meta.versionId` that is a valid FHIR `id`
is answered as a weak `ETag` (`W/"3"` for version `3`), and a
`meta.lastUpdated` that is a valid FHIR `instant` as `Last-Modified`, an
HTTP-date in GMT, so you can make a conditional read. Either one absent, or
of another form, gives no such header; the gateway logs the second case by
its shape and never logs the value (`fhir/prescriptions/version.rs`). A
member that answers another prescription than the one asked for is never
passed through
(`fhir/prescriptions/read.rs`: `a_member_answering_another_prescription_is_never_passed_through`).

## Receiving a dispensation

`POST {fhir-base}/MedicationDispense` with `application/fhir+json` takes one
`MedicationDispense`, the FHIR R4 `create` interaction, and the
`composition-` create scope over every template (`user/composition-*.c`,
for example). The gateway writes it to the one member that owns the
prescription it names, and to no other member.

The dispensation is checked before any member is asked. A body that is no
`MedicationDispense` is a `400`; one that breaks `MedicationDispense-eu-mpd`,
or a contained `Medication` that breaks `Medication-eu-mpd`, is a `422` with
one issue per finding (`fhir/prescriptions/dispense.rs`:
`a_dispensation_that_breaks_its_profile_asks_no_member`). Its `subject`
names the patient by an identifier, either a logical reference or a
contained `Patient` that `subject` references by `#id`, and each identifier
whose system is in `receive_namespaces` is resolved. Every one of them must
name the same `ehr_id` at every member, or the dispensation is refused
`422`. The dispensation must carry its own business identifier, an
`identifier` with a `system` and a `value`, or it is refused `422`
`required`, naming `MedicationDispense.identifier`
(`a_dispensation_with_no_business_identifier_is_refused_naming_the_element`):
it is how the gateway tells a dispensation sent again from a new one.

Each `authorizingPrescription` names a prescription by its business
identifier, and may carry the face's reference `MedicationRequest/{id}`
beside it, which must then name the same prescription
(`a_dispensation_by_identifier_and_the_faces_reference_reaches_its_owner`,
`a_reference_and_an_identifier_that_disagree_write_nothing`). A
prescription named by the face's reference alone is refused `422`, "name
the prescription by its identifier; the member cannot resolve the
gateway's reference": the member receives your body unchanged, so it can
link the dispensation only by the identifier it holds
(`a_dispensation_naming_its_prescription_by_the_faces_reference_alone_is_refused`).

The gateway then asks every member declared for `Electronic-Prescriptions`
that holds the patient for the patient's prescriptions, as the search does,
by the member's own `ehr_id` alone, and finds the named prescriptions in the
answers itself. The prescription identifier never reaches a member. Every
member asked must answer, because a silent one could be the owner
(`a_member_silent_at_the_lookup_refuses_the_dispensation`). The dispensation
is first checked against the owner's dispensations: the gateway asks the
owner's own endpoint for the patient's dispensations, by its own `ehr_id`
alone, and looks for one with the received dispensation's business
identifier itself, so the identifier never reaches the member in a query. A
dispensation the owner already holds is refused `409` `duplicate`, naming
the one already written, and nothing is written; this is how a dispensation
you send again after a time-out is never written twice
(`a_dispensation_the_owner_already_holds_is_refused_as_a_duplicate_with_nothing_written`,
`another_dispensation_of_the_patient_is_no_duplicate_and_is_written`). An
owner that does not answer that read refuses the dispensation, because a
duplicate cannot be ruled out
(`an_owner_that_does_not_answer_the_duplicate_check_writes_nothing`). The
dispensation is then created at the owner's own endpoint with
`POST {own}/MedicationDispense`, once, its body exactly as you sent it
(`a_dispensation_reaches_the_owner_alone_by_its_ehr_id_with_its_body_unchanged`).
The member decides whether its prescription still admits the dispensation.
The gateway holds no lock, and it never sends a dispensation again
(`an_owner_that_does_not_answer_is_asked_once_and_the_outcome_is_unknown`).

An `openEHR-federation-endpoint` header is a check only: when it names an
endpoint of another member than the owner, nothing is written
(`a_header_naming_another_member_writes_nothing`).

A dispensation with no owner answers as an unknown patient does, and
nothing is written in any of the refusals below
(`no_owner_answers_as_an_unknown_patient_does`,
`a_prescription_two_members_hold_is_refused_with_an_integrity_incident`,
`two_matches_at_one_member_are_refused_with_no_incident`,
`prescriptions_held_by_different_members_write_nothing`). The owner's own
refusal is passed on as `409`, naming the member and its status
(`the_owners_refusal_is_a_conflict_naming_it`).

A withdrawal is a dispensation sent again with `status`
`entered-in-error`, routed as any other; it has no route of its own.

## Reading a dispensation

`GET {fhir-base}/MedicationDispense/{id}` reads a dispensation by the
[locator id](#the-locator-ids) its receipt's `Location` named, and takes the
`composition-` read scope over every template. The gateway asks the member
the id names alone, with
`GET {own}/MedicationDispense?patient.identifier={ehr_id_system}|{ehr_id}&_id={member's id}`,
and answers the member's `MedicationDispense` as it sent it, with its `id`
the one you read, its `ETag` and `Last-Modified` from the member's
`meta.versionId` and `meta.lastUpdated`, and the member named in
`openEHR-federation-endpoint` (`fhir/prescriptions/dispensed.rs`:
`the_dispensation_a_receipt_located_is_read_at_its_member_by_its_ehr_id`,
`a_dispensation_id_with_no_owner_is_not_found_and_asks_no_member`,
`a_member_that_does_not_answer_the_dispensation_read_is_named`). Its other
answers are a prescription read's, with "dispensation" in their text.

## Reading a version

`GET {fhir-base}/MedicationDispense/{id}/_history/{vid}` and
`GET {fhir-base}/MedicationRequest/{id}/_history/{vid}` are the FHIR R4
`vread`, so the `Location` of a receipt that carries the member's version
reads back the dispensation it named. The gateway reads the resource as
the read does, with the same scope, headers, answers and access record,
and answers it when the member's `meta.versionId` is `{vid}`. The gateway
keeps no history, so any other version, and a resource with no
`meta.versionId`, is `404` `not-found`. A `{vid}` that is no FHIR `id` is
refused before any member is asked (`fhir/prescriptions/vread.rs`:
`the_location_a_receipt_answered_reads_back_the_dispensation_it_located`,
`a_version_the_member_does_not_hold_now_is_not_found`,
`a_version_that_is_no_fhir_id_asks_no_member`,
`a_prescription_is_read_at_the_version_it_holds_and_no_other`).

## The locator ids

The `id` the face gives a prescription, and the `id` in a received
dispensation's `Location`, name three things: the member's own endpoint,
the member's `ehr_id` for the patient and the member's own `id` of the
resource. The gateway packs them into the 64 characters a FHIR R4 `id`
allows: a form byte, the first 6 bytes of the SHA-256 of the endpoint's
`endpoint_id`, then the `ehr_id` and the member's `id`, each as its 16 bytes
when it is a UUID and as its text otherwise, written in the base64url
alphabet (RFC 4648 §5) with `.` in place of `_`. The gateway holds no state
between the search and the read, and the same prescription always gets the
same id (`app/ferrofed-server/src/fhir/locator.rs`:
`an_id_is_a_fhir_id_and_reads_back_to_its_parts`,
`the_id_is_the_same_each_time`).

The id only locates a resource. It carries no patient identifier, and every
read is still held to the caller's grant and asked of the member by its
`ehr_id`. An id the gateway did not write, or one naming an endpoint not
declared for the category, answers `404` with the text every id with no
owner gets. An id whose endpoint part matches two declared endpoints is a
`409`, and the gateway never chooses between them.

A prescription whose parts do not fit in 64 characters, or whose member
`id` is no FHIR `id`, keeps the member's own `id` in the search and cannot
be read through the face. The search then adds a `warning` `outcome` entry
naming the member and quoting no identifier
(`fhir/prescriptions/read.rs`:
`a_prescription_with_no_id_to_read_it_by_is_listed_and_warned_of`). A
created dispensation whose member names no such `id` gets a `201` with no
`Location`, and the `OperationOutcome` says so.

## Every answer

Every answer other than a success is an `OperationOutcome`. The gate's
authentication answers (`401` and the `403`s of a token) are those of every
route ([Failure statuses](../operate/authentication.md#failure-statuses)),
and another method on any of these paths is refused `403`
`operation-refused` before the token is read (`fhir/serving.rs`:
`another_method_on_a_prescription_path_is_refused_at_the_gate`).

| Route | Status | When |
|---|---|---|
| search | `200` | The `searchset`, empty for a patient no declared member holds or none may say anything about. |
| read | `200` | The member's prescription or dispensation, with the headers [Reading one prescription](#reading-one-prescription) names. |
| receipt | `201 Created` | The owner created the dispensation. `Location` names it by its locator id, `{fhir-base}/MedicationDispense/{id}`, with `/_history/{version}` when the member named a version that is a FHIR `id`, which the [version read](#reading-a-version) takes, and the `OperationOutcome` names the member and every profile constraint the check did not evaluate. |
| search | `400` | A parameter other than `patient.identifier`, `_count`, `-offset` and `_format`, no patient, or two. |
| receipt | `400` | A body that is no `MedicationDispense` or no UTF-8 text; or an `openEHR-federation-endpoint` header naming another member's endpoint, or one the registry does not hold. |
| every route | `403` | No granted scope covers the route, or a `patient/` grant reaches beyond its patient (`patient-confinement`). |
| every route | `404` `not-supported` | No member is declared for the route's category. |
| read | `404` `not-found` | The gateway did not write the id, it names no declared member, the gateway already places its `ehr_id` at another member, or the member holds no such resource for that `ehr_id`. Every case gets the same text. |
| version read | `404` `not-found` | As a read; or the member holds the resource at another version, or with no `meta.versionId`, and the gateway keeps no history. |
| receipt | `404` `not-found` | No member holds the patient, none may say so, or no member holds the prescription. Every case gets the same text. |
| read | `409` `conflict` | Two members claim the `ehr_id` the id names, which also raises the integrity incident; the id's endpoint part matches two endpoints; or the member answered more than one resource for one id, with no incident. Nothing is passed on. |
| receipt | `409` `conflict` | Two members hold the prescription, which also raises the `PrescriptionIdentifierCollision` integrity incident; one member holds it twice, with no incident; or the owner refused the dispensation, named with its status. |
| receipt | `409` `duplicate` | The owner already holds a dispensation with the received one's business identifier, named by its locator id. Nothing was written. |
| receipt | `415` | Another media type than `application/fhir+json`. |
| receipt | `422` | The dispensation breaks its profiles; it carries no business identifier with a system and a value (`required`); the patient's identifiers disagree; the prescriptions are held by different members; a prescription is named by the face's reference alone; a reference and an identifier name different prescriptions; a reference is not to the face; or the owner is not declared for `Electronic-Dispensations`. |
| search, read | `424` | A member answered with an error, with no `searchset`, or with another resource than the one asked for; or the patient could not be resolved at a member. |
| receipt | `500` | The dispensation profiles this build carries could not be read; or the request to the owner would have carried a patient identifier outside its body, so it was not sent (§5.4.1, N33). |
| receipt | `502` | The owner answered the dispensation with a `5xx` status or a `401`, failed in another way, or could not be reached. For an owner that could not be reached, whether it wrote the dispensation is unknown. |
| every route | `503` `access-unrecorded` | The access record has no place, so the answer is withheld ([Failing closed](../operate/access-log.md#failing-closed)). A receipt reserves its record's place first, so a receipt refused this way reached no member. |
| receipt | `503` | A member that could hold the prescription answered the lookup with no usable `searchset`, or the patient could not be resolved at it; the owner answered the duplicate check with an error; or the dispensation could not be sent before the deadline. Nothing was written. |
| search, read | `504` | A member is offline or did not answer in time. |
| receipt | `504` | A member did not answer the lookup in time, and nothing was written; or the owner did not answer the dispensation in time, and whether it wrote it is unknown. |

After a `504` or a `502` for an owner that could not be reached, the
gateway did not send the dispensation again: ask the member, or read the
prescription's status, before you send it once more with the same dispense
identifier, which the member refuses when it already holds it. No other
answer follows a write the owner could have made.

## Completeness

A search asks every declared member that holds the patient, and is
all-or-nothing by default, as a federated query is (§11.3, §11.4). A member
that does not answer, answers with no `searchset`, or links a page outside
its own base fails the search: `504` for a member that is offline or timed
out, `424` otherwise, with every such member named. A member the identifier
cross-reference could not answer for fails it too, named `not-resolved`
(`fhir/prescriptions/mod.rs`: `a_silent_member_fails_the_search_and_is_named`,
`an_unreachable_member_fails_the_search_with_a_gateway_time_out`;
`fhir/prescriptions/resolution.rs`:
`a_member_the_cross_reference_could_not_answer_for_fails_the_search_and_is_named`).

Where the deployment sets `[federation] best_effort = true`, the default,
send `openEHR-federation-completeness: partial` to take the prescriptions
of the members that answered. Each member that did not is then named in a
`warning` `outcome` entry with its §11.1 status, so a partial list never
reads as a complete one (`fhir/prescriptions/mod.rs`:
`under_partial_a_silent_member_is_named_in_an_outcome_entry`;
`fhir/prescriptions/resolution.rs`:
`under_partial_a_member_whose_resolution_failed_is_named_in_an_outcome_entry`).

A read asks one member, so it has no partial answer. A receipt has none
either: the lookup needs every member that could own the prescription, and
`partial` does not change that.

## Remaining dispensations

No EU element carries the number of dispensations a prescription has left:
the MPD element `dispenseRequest.numberOfRepeatsAllowed` is the number
authorised. A member states the remaining number in the extension
`https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` on
`MedicationRequest.dispenseRequest`, a `valueUnsignedInt`, at most once, and
leaves it out when it states none:

```json
"dispenseRequest": {
  "extension": [{
    "url": "https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations",
    "valueUnsignedInt": 4
  }],
  "numberOfRepeatsAllowed": 5
}
```

The gateway passes the extension through as the member sent it, as it
passes every element, and never computes or corrects the number. Its FHIR
R4 `StructureDefinition` is published at that URL, and with a `.json`
suffix beside it, from `src/medication/StructureDefinition-remaining-dispensations.json`
of the published `eehrxf` crate
([The definitions behind FerroFED's URLs](../operate/registry-fhir.md#the-definitions-behind-ferrofeds-urls);
FerroBRIDGE's [`crates/eehrxf/tests/it/medication/extension.rs`](https://github.com/FerroHEALTH/FerroBRIDGE/blob/384882893fb8dba3fc1f0a43c0e8f04bbba1771d/crates/eehrxf/tests/it/medication/extension.rs):
`the_extension_is_simple_at_most_once_with_an_unsigned_int`,
`the_context_is_the_dispense_request_of_a_medication_request`).

## Access records and the counter

Each request that reached a member writes one access record
([The access log](../operate/access-log.md)):

| Request | Recorded as | Test |
|---|---|---|
| search | a read under `Electronic-Prescriptions`, naming every member asked | `fhir/prescriptions/restricted.rs`: `a_search_is_recorded_once_under_electronic_prescriptions_naming_each_member` |
| prescription read | a read under `Electronic-Prescriptions`, naming the member and the `ehr_id` it was asked by | `fhir/prescriptions/read.rs`: `a_read_is_recorded_once_under_electronic_prescriptions_naming_its_member` |
| receipt | a creation under `Electronic-Dispensations` and `Electronic-Prescriptions`, naming every member the lookup asked and the owner | `fhir/prescriptions/dispense.rs`: `a_dispensation_is_recorded_as_a_creation_of_both_categories` |
| dispensation read | a read under `Electronic-Dispensations` | `fhir/prescriptions/dispense.rs`: `a_dispensation_read_is_recorded_as_a_read_of_dispensations` |
| version read | a read as the read of the same resource records it | `fhir/prescriptions/dispense.rs`: `a_dispensation_version_read_is_recorded_as_a_read_of_dispensations` |

A not-served answer, and a request refused before any member was asked,
write no record. A receipt takes its record's place before the owner is
asked, so a receipt whose record has no place is refused `503
access-unrecorded` and reaches no member
(`a_dispensation_whose_record_has_no_place_reaches_no_member`). The
patient identifier you sent never reaches the gateway's log
(`fhir/prescriptions/restricted.rs`: `the_patient_identifier_never_reaches_the_log`).

Every receipt that reaches the lookup is counted once by how it ended in
`ferrofed_fhir_dispensations_total`, labelled `outcome` alone: `created`,
`unowned`, `claimed`, `several`, `unroutable`, `silent`, `refused`,
`unknown` or `failed`. No prescription, patient or member is ever a label,
so a dispensation no member owns is counted with no value
(`fhir/prescriptions/dispense.rs`:
`a_dispensation_with_no_owner_is_counted_with_no_value`;
[The metrics](../operate/metrics.md#the-metrics)). A prescription two
members hold also raises the integrity incident
`PrescriptionIdentifierCollision`, which carries no prescription identifier
([The registry](../operate/registry.md)).

## The capability statement

`GET {fhir-base}/metadata` lists `MedicationRequest` only when a member is
declared for `Electronic-Prescriptions`, with the profile
`MedicationRequest-eu-mpd`, the `search-type`, `read` and `vread`
interactions and the `patient.identifier` search parameter. It lists
`MedicationDispense` only when a member is declared for
`Electronic-Dispensations`, with the profile `MedicationDispense-eu-mpd` and
the `create`, `read` and `vread` interactions. Each interaction it declares is one its route serves
(`fhir/serving.rs`:
`with_no_member_declared_the_capability_statement_lists_neither_resource`,
`a_declared_member_lists_both_resources_in_their_mpd_profiles`,
`every_interaction_declared_for_a_prescription_or_dispensation_is_one_the_route_serves`).
