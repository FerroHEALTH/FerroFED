<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Rights of natural persons

Annex II, point 1.3 of Regulation (EU) 2025/327 asks that the harmonised
software components of an EHR system ensure "its interoperability, safety
and security features uphold the rights of natural persons". Chapter II,
Section 1 grants those rights for primary use. This page holds each right
against FerroFED's two harmonised components, the logging component (the
[access log](../operate/access-log.md)) and the interoperability component
(the [patient summary over FHIR](../integrate/patient-summary.md) and
[receiving a document](receiving-documents.md)), and against the gateway
around them where a right reaches it.

Most of these rights are exercised through a service FerroFED is not. The
Member State's electronic health data access service (Art 4) gives a person
access, a download, insertion, rectification and the information on access.
Its health professional access service (Art 12) and the healthcare provider
transmit and restrict. FerroFED offers patients no access service of its
own ([Intended purpose](regulatory-status.md#intended-purpose)). For each
right the page says who acts, what the components do for it with the tests
that show it, or the gap and the issue that closes it.

Every quotation is from the Official Journal text, vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
Tests are named by file under `app/ferrofed-server/tests/it/` and function.
This page is not legal advice.

## Summary

| Right | Who acts | FerroFED | Status |
|---|---|---|---|
| Art 3(1), access | the access service | the summary is served to a professional; no test serves it to a person's own grant | partial |
| Art 3(2), a download in the exchange format | the access service | patient summaries only | partial |
| Art 4(2), a proxy or a legal representative | the proxy service | admits the token its issuer grants and records who acted | not the gateway's |
| Art 5, inserting information | the access service and the member CDR | a person's grant writes only into its own EHR; nothing marks the entry as the person's | partial |
| Art 6, rectification | the access service and the controller | routes the controller's correction | not the gateway's |
| Art 7(1), transmission to another provider | the healthcare provider | adds no fee, limit or format | met |
| Art 7(2), cross-border transmission | the national contact point | the national route is planned | partial |
| Art 7(3), transmission to social security | the healthcare provider | outside the intended purpose | not the gateway's |
| Art 7(4), sending a downloaded copy | the receiving provider | receives patient summaries only | partial |
| Art 8, restricting access | the person, the consent service and the member | carries the restriction out and hides its fact | met |
| Art 9, information on access | the access service | records every access for at least three years and serves the search | met |
| Art 10, opting out | the Member State | an opt-out is carried out as a restriction is | not the gateway's |
| Art 11(5), an emergency access logged for the person | the healthcare provider and the member | marks the record | met |
| Art 16(1), electronic identification | the access service | not an access service | not the gateway's |
| Art 18, no fee | every provider and third party | charges a person nothing | met |
| Art 21, a complaint | the digital health authority | not a complaint body | not the gateway's |

## Art 3: access and a download

Art 3(1): "Natural persons shall have the right to access at least personal
electronic health data relating to them that belong to the priority
categories referred to in Article 14 ... through the electronic health data
access services referred to in Article 4." Access "shall be provided
immediately after the personal electronic health data have been registered
in an EHR system". Art 3(2) gives a right "to download free of charge an
electronic copy ... in the European electronic health record exchange
format referred to in Article 15".

**Who acts.** The access service of Art 4. Art 3(3) lets a Member State
delay access, which is that service's to apply.

**What FerroFED does.** The gateway holds no clinical data of its own and
reads each member at the time of the request, so what a member registered
is in the next answer ([Intended purpose](regulatory-status.md#intended-purpose)).
A token confined to one patient reads that patient's EHR at every member
and nothing else ([Patient grants](../operate/authentication.md#patient-grants);
`auth/patient.rs`: `the_token_s_own_patient_is_admitted_at_every_member`,
`a_query_for_another_patient_is_refused`). The interoperability component
serves the patient summary in the HL7 Europe format
([Asking for a summary](../integrate/patient-summary.md#asking-for-a-summary);
`fhir/summary.rs`:
`the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node`).

**The gap.** The summary is served to a caller that acts for a
professional and states a purpose of use (`fhir/refusals.rs`:
`a_client_that_names_no_professional_is_refused`,
`a_token_with_no_purpose_of_use_is_refused`). No test shows it served to a
person's own grant or to an access service acting for the person. The other
five categories of Art 14(1) have no interface in the exchange format
(Annex II, point 2.1:
[#522](https://github.com/FerroHEALTH/FerroFED/issues/522),
[#523](https://github.com/FerroHEALTH/FerroFED/issues/523),
[#666](https://github.com/FerroHEALTH/FerroFED/issues/666),
[#667](https://github.com/FerroHEALTH/FerroFED/issues/667),
[#668](https://github.com/FerroHEALTH/FerroFED/issues/668)). The first gap
is held by [#961](https://github.com/FerroHEALTH/FerroFED/issues/961) until
its own issue is filed.

## Art 4(2): proxies and legal representatives

Art 4(2)(a) has proxy services enable "natural persons to authorise other
natural persons of their choice to access their personal electronic health
data", and (b) lets "legal representatives of natural persons ... access
personal electronic health data of those natural persons whose affairs they
administer".

**Who acts.** The Member State's proxy service, which decides the
authorisation.

**What FerroFED does.** The gateway admits the token the issuer grants and
does not judge the authorisation behind it. The access record names the
natural person the token names, by its issuer and subject
([The person behind an access](../operate/access-log.md#the-person-behind-an-access);
`access/accessor.rs`:
`a_person_is_recorded_with_the_professional_and_the_verified_level`). When
the proxy service's issuer names the representative in the token, the
record names the representative.

## Art 5: inserting information

Art 5: persons "shall have the right to insert information in the EHR of
those natural persons through electronic health data access services or
applications linked to those services". The information "shall be clearly
distinguishable as having been inserted by the natural person or by his or
her representative", and they "shall not be able to directly alter the
electronic health data and related information inserted by health
professionals".

**Who acts.** The access service or an application linked to it writes; the
member CDR stores the entry and knows who wrote each version.

**What FerroFED does.** The interoperability component takes a document and
creates a new composition at the declared member. It never updates or
deletes one, so it cannot alter a professional's entry
([Written to the declared member](receiving-documents.md#written-to-the-declared-member);
`fhir/receive/write.rs`:
`a_document_is_written_to_its_member_by_its_ehr_id_alone`). A token
confined to one patient writes only into that patient's EHR
(`fhir/receive/write.rs`:
`a_caller_confined_to_one_patient_writes_only_into_its_ehr`). Under a
patient grant, the caller token the gateway signs tells a node the patient
scope that covered the request, which a query test shows
(`auth/patient.rs`:
`each_node_is_told_its_own_ehr_id_and_the_covering_patient_scope`). Each
receipt is recorded with the caller who sent it
(`fhir/receive/audit.rs`:
`a_receipt_is_recorded_once_as_a_creation_of_its_category`).

The routed ITS-REST writes are outside the harmonised components. Under a
patient grant whose scopes cover an update or a delete, the gateway sends it
to the member as for any caller, and the member, which records who wrote
each version, decides whether the person may change it.

**The gap.** Nothing the component writes marks the composition as
inserted by the person, and no test shows the member or the composition
telling a person's entry from a professional's. Held by
[#961](https://github.com/FerroHEALTH/FerroFED/issues/961) until its own
issue is filed.

## Art 6: rectification

Art 6: access services "shall enable natural persons to easily request
online the rectification of their personal electronic health data in
accordance with Article 16 of Regulation (EU) 2016/679. Where appropriate,
the controller shall verify with a relevant health professional the
accuracy of the information provided in the request."

**Who acts.** The access service takes the request and the controller, the
healthcare provider whose CDR holds the record, corrects it.

**What FerroFED does.** A correction is a write by the controller, which
the gateway routes to the member that controls the record and records with
the version it left
([What a record names](../operate/access-log.md#what-a-record-names);
`access/written.rs`:
`a_composition_update_records_the_new_version_never_the_one_it_amends`).

## Art 7: portability

Art 7(1): persons may "give access to, or to request a healthcare provider
to transmit, all or part of their personal electronic health data to
another healthcare provider of their choice immediately, free of charge and
without hindrance from the healthcare provider or from the manufacturers of
the systems used by that healthcare provider".

**Who acts.** The healthcare provider transmits; the manufacturer must not
hinder it.

**What FerroFED does.** A record stays at the member that holds it, and the
gateway adds no fee, no export limit and no format of its own
([Art 7(1): transmission without hindrance](claims-review.md#art-71-transmission-without-hindrance)).
The routing state it learned is exported for a replacement product
([What is exported and what is not](../operate/identity-state-export.md#what-is-exported-and-what-is-not);
`identity_export.rs`:
`the_export_holds_the_learned_creating_system_mappings_in_its_format`).

Art 7(2): where the providers are in different Member States, persons may
"request the transmission of their personal electronic health data in the
European electronic health record exchange format ... through the
cross-border infrastructure referred to in Article 23".

**Who acts.** The national contact point for digital health. FerroFED does
not reach another Member State itself. It serves a request a contact point
relays, and the record names the professional of the other Member State
([A request a national contact point relays](../operate/access-log.md#a-request-a-national-contact-point-relays);
`access/relayed.rs`:
`the_record_names_the_relayed_professional_and_provider_as_asserted`).
Answering a contact point's request for a patient's documents is planned
([Cross-border access](regulatory-status.md#cross-border-access),
[#524](https://github.com/FerroHEALTH/FerroFED/issues/524)).

Art 7(3): persons may request "a healthcare provider to transmit a part of
their personal electronic health data to a clearly identified recipient in
the social security or reimbursement services sector".

**Who acts.** The healthcare provider. A social security recipient is
outside FerroFED's users, which are healthcare providers in patient care
([Intended purpose](regulatory-status.md#intended-purpose)).

Art 7(4): persons who downloaded a copy "shall be able to transmit those
data to healthcare providers of their choice in the European electronic
health record exchange format ... The receiving healthcare provider shall
accept such data and be able to read them, as applicable."

**Who acts.** The receiving provider, whose EHR system must receive the
format (Annex II, point 2.2).

**What FerroFED does.** It receives patient summaries, from a person's own
grant too (`fhir/receive/write.rs`:
`a_caller_confined_to_one_patient_writes_only_into_its_ehr`). A category it
cannot receive is refused, naming it (`fhir/receive/refused.rs`:
`a_category_no_member_receives_is_refused_naming_it`). The other five
categories are the gap of point 2.2
([#665](https://github.com/FerroHEALTH/FerroFED/issues/665),
[#523](https://github.com/FerroHEALTH/FerroFED/issues/523),
[#666](https://github.com/FerroHEALTH/FerroFED/issues/666),
[#667](https://github.com/FerroHEALTH/FerroFED/issues/667),
[#668](https://github.com/FerroHEALTH/FerroFED/issues/668)).

## Art 8: restricting access

Art 8: "Natural persons shall have the right to restrict the access of
health professionals and healthcare providers to all or parts of their
personal electronic health data", and "The fact that a natural person has
restricted access under the first paragraph shall not be visible to
healthcare providers."

**Who acts.** The person sets the restriction under the Member State's
rules; a consent service or the member CDR holds it.

**What FerroFED does.** A member the consent pre-filter excludes is never
sent the request, and a node's own consent refusal contributes no rows
([Consent](../operate/consent.md); `consent.rs`:
`a_member_the_prefilter_denies_is_reported_and_never_contacted`). Under
`[federation.consent] disclose = false`, which an EU deployment sets, the
exclusion and the refusal read exactly as a member that does not know the
patient
([Withholding consent exclusions](../operate/consent-exclusions.md);
`consent_withheld/mod.rs`:
`a_withheld_exclusion_reads_exactly_as_a_member_that_does_not_know_the_patient`;
`consent_withheld/node.rs`:
`a_node_refusal_reads_exactly_as_a_member_that_does_not_know_the_patient`).
A request a national contact point relays is always served that way
(`contact_point/withheld.rs`:
`a_restricted_patient_and_an_unknown_one_answer_alike`). The default,
`disclose = true`, follows the Federation Tier's `consent-denied` (N27a), so
a deployment in the EU must change it.

## Art 9: information on access

Art 9(1): persons have "the right to obtain information, including through
automatic notifications, on any access to their personal electronic health
data through the health professional access service". Art 9(2): it is
provided "through electronic health data access services and shall be
available for at least three years from each date of access", naming at
least the provider or individuals who accessed the data, the date and time,
and "which personal electronic health data were accessed".

**Who acts.** The access service gives the information and sends any
notification.

**What FerroFED does.** The logging component records every access with the
caller, the time, the categories and the origins, and refuses an access it
cannot record ([What a record names](../operate/access-log.md#what-a-record-names);
[Failing closed](../operate/access-log.md#failing-closed);
`access/routed.rs`:
`every_access_whose_record_the_spool_cannot_take_is_refused_with_no_data`).
A record names the patient behind an `ehr_id` the request reached
([The patient behind an `ehr_id`](../operate/access-log.md#the-patient-behind-an-ehr_id);
`access/patient.rs`:
`a_read_by_ehr_id_names_the_patient_the_identity_binding_holds`), and is
kept three years by default
([How long a record is kept](../operate/access-log.md#how-long-a-record-is-kept);
`access/retention.rs`:
`with_no_retention_table_a_record_is_kept_three_years`,
`a_retention_under_three_years_is_refused`). The access service finds a
person's records with an ITI-81 search at the Audit Record Repository
([Reading the log](../operate/access-log.md#reading-the-log);
`access/search.rs`: `the_persons_search_finds_every_access_to_their_data`).
The gateway sends no notification of its own.

## Art 10: opting out

Art 10(1): "Member States' laws may provide that natural persons have the
right to opt out from the access to their personal electronic health data
registered in an EHR system through the electronic health data access
services referred to in Articles 4 and 12."

**Who acts.** The Member State, which sets the mechanism and its
safeguards (Art 10(2)).

**What FerroFED does.** No mechanism of its own. An opt-out the consent
service or a member reports is carried out as a restriction is (Art 8
above), and an access in the vital interests that Art 10(2) may allow is
marked as under Art 11(5) below.

## Art 11(5): an emergency access

Art 11(5): access to restricted data may be granted "where necessary in
order to protect the vital interests of the data subject. Such cases shall
be logged in a clear and understandable format and shall be easily
accessible for the data subject."

**Who acts.** The professional asserts the emergency and the member decides
what to release.

**What FerroFED does.** A record whose token declares a configured
emergency purpose carries a mark the access service can search for
([Emergency access](../operate/access-log.md#emergency-access);
`access/emergency.rs`: `a_declared_emergency_purpose_marks_the_record`,
`the_emergency_purpose_is_conveyed_to_the_node`). Under
`emergency = "pass-to-node"` the record names each member whose consent
denial was set aside (`access/emergency_consent.rs`:
`an_emergency_request_asks_the_denied_member_and_records_the_set_aside`).

## Art 16(1): electronic identification

Art 16(1): where persons use access services, they "shall have the right to
identify themselves electronically using any electronic identification
means which are recognised pursuant to Article 6 of Regulation (EU) No
910/2014".

**Who acts.** The access service and its identity provider. The gateway
verifies the tokens of the issuers you configure and authenticates no person
itself ([Client authentication](../operate/authentication.md)).

## Art 18: no fee

Art 18: "A healthcare provider or a third party shall not directly or
indirectly charge data subjects a fee or costs, or require compensation,
for sharing or accessing data."

**What FerroFED does.** It has no fee, payment or quota per person
([Art 7(1): transmission without hindrance](claims-review.md#art-71-transmission-without-hindrance)).

## Art 21: a complaint

Art 21(1): persons "shall have the right to lodge a complaint in relation
to the provisions laid down in this Chapter ... with the competent digital
health authority".

**Who acts.** The digital health authority. The manufacturer's own
complaint handling is [Complaints and incidents](post-market.md).

## The rest of Chapter II

Art 11(1) to (4), Art 12 to 15, 17, 19, 20 and 22 to 24 place obligations on
Member States, health professionals, healthcare providers, the Commission
and the authorities, and grant no right of their own to a natural person.
Where they bind an EHR system, the [Annex II checklist](annex-ii.md) holds
them.
