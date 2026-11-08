<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Annex II checklist

Annex II of Regulation (EU) 2025/327 on the European Health Data Space (the
EHDS Regulation) lays down the "Essential requirements for the harmonised
software components of EHR systems". Art 30(1)(a) has the manufacturer
ensure that those components "are in conformity with the essential
requirements laid down in Annex II", and Art 37(2) has the technical
documentation "demonstrate that the EHR system complies with the essential
requirements laid down in Annex II".

This page is FerroFED's checklist of every point and sub-point of Annex II,
by the Regulation's own numbering: whether FerroFED meets it, the page and
the tests that show it, and the issue that closes any gap. It is kept as
[`conformance/annex-ii.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/annex-ii.tsv),
and `scripts/checks/annex-ii.sh` fails a change that leaves a point without
a row, quotes a requirement that is not in that point's text of the vendored
Regulation, names a page heading or a test that does not exist, or leaves a
gap without its issue. CI runs it on every change.

Every quotation of the Regulation is from the Official Journal text,
vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice.

## How to read it

| Status | Meaning |
|---|---|
| met | the evidence shows the requirement met for what this release does |
| partial | part of the requirement is met; the note says what is missing and the issues close it |
| planned | nothing is built yet; the issues hold the work |
| not-applicable | the requirement does not bind FerroFED; the note says why |

The statuses are read against the Regulation's text alone. The common
specifications for the essential requirements (Art 36(1)) and the exchange
format (Art 15(1)) are not adopted yet
([#799](https://github.com/FerroHEALTH/FerroFED/issues/799), [FerroHEALTH/FerroBRIDGE#1327](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1327)), and each can
add to what a point asks. When one is adopted, every point it touches is
checked again, the access log against the common specifications among them
([#799](https://github.com/FerroHEALTH/FerroFED/issues/799)).

The technical documentation cites this checklist for Annex III, point 4
([Technical documentation](technical-documentation.md#the-annex-iii-index)).
That point is marked documented only when every row below is met or
not-applicable.

The access log's own mapping of each 3.2 item to the element of the record
that carries it is [the Annex II 3.2 checklist](../operate/access-log.md#the-annex-ii-32-checklist)
of the access log page.

The tables and the notes below are rendered from the TSV by
`scripts/conformance/annex-ii.sh --write`, and the guard fails a page that
differs from it. Every issue a row names is open and tracks that row's gap.

<!-- annex-ii:begin -->
<!-- Rendered from conformance/annex-ii.tsv by scripts/conformance/annex-ii.sh --write. Do not edit between the markers. -->

## 1. General requirements

| Point | Requirement | Evidence | Status | Issue |
|---|---|---|---|---|
| 1.1 | "shall achieve the performance intended by its manufacturer" | [The logging component](clinical-safety.md#the-logging-component); [The interoperability component](clinical-safety.md#the-interoperability-component); [Overall residual risk](clinical-safety.md#overall-residual-risk); [Review and change](clinical-safety.md#review-and-change) | partial | [#1726](https://github.com/FerroHEALTH/FerroFED/issues/1726), [FerroHEALTH/FerroBRIDGE#1694](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1694), [FerroHEALTH/FerroBRIDGE#1696](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1696), [FerroHEALTH/FerroBRIDGE#1700](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1700), [FerroHEALTH/FerroBRIDGE#1701](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1701) |
| 1.2 | "can be supplied and installed, taking into account the instructions and information provided by the manufacturer" | [Installation](../operate/instructions-for-use.md#installation); [9. Check, start and admit](../operate/production.md#9-check-start-and-admit); [`scripts/checks/production-guide.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/production-guide.sh); [The compatibility policy](../operate/upgrading.md#the-compatibility-policy) | met | - |
| 1.3 | "its interoperability, safety and security features uphold the rights of natural persons" | [Summary](natural-person-rights.md#summary); [Art 8: restricting access](natural-person-rights.md#art-8-restricting-access); [Art 9: information on access](natural-person-rights.md#art-9-information-on-access); `app/ferrofed-server/tests/it/consent_withheld/mod.rs` (`a_withheld_exclusion_reads_exactly_as_a_member_that_does_not_know_the_patient`); `app/ferrofed-server/tests/it/access/search.rs` (`the_persons_search_finds_every_access_to_their_data`); `app/ferrofed-server/tests/it/fhir/receive/write.rs` (`a_caller_confined_to_one_patient_writes_only_into_its_ehr`); `app/ferrofed-server/tests/it/access/emergency.rs` (`a_declared_emergency_purpose_marks_the_record`); [Art 5: inserting information](natural-person-rights.md#art-5-inserting-information); `app/ferrofed-server/tests/it/fhir/receive/inserted.rs` (`a_persons_own_document_is_marked_as_inserted_by_them_with_no_identifier_added`); `app/ferrofed-server/tests/it/fhir/receive/laboratory_grant.rs` (`a_persons_own_grant_lists_and_reads_their_laboratory_report`); `app/ferrofed-server/tests/it/fhir/receive/laboratory_grant.rs` (`a_persons_own_grant_asking_for_another_patients_laboratory_reports_is_refused`) | partial | [#1405](https://github.com/FerroHEALTH/FerroFED/issues/1405), [#1406](https://github.com/FerroHEALTH/FerroFED/issues/1406), [FerroHEALTH/FerroBRIDGE#1325](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1325), [FerroHEALTH/FerroBRIDGE#1326](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1326) |
| 1.4 | "interoperability and compatibility are reliable and secure" | [B2: gateway to member node](threat-model.md#b2-gateway-to-member-node); [`scripts/checks/fhir-validator.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/fhir-validator.sh); `app/ferrofed-server/tests/it/fhir/summary.rs` (`the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node`) | partial | [#675](https://github.com/FerroHEALTH/FerroFED/issues/675), [FerroHEALTH/FerroBRIDGE#1327](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1327) |

## 2. Requirements for interoperability

| Point | Requirement | Evidence | Status | Issue |
|---|---|---|---|---|
| 2.1 | "it shall provide an interface enabling access to the personal electronic health data processed by it in the European electronic health record exchange format" | [Asking for a summary](../integrate/patient-summary.md#asking-for-a-summary); [The document list](../integrate/patient-summary.md#the-document-list); `app/ferrofed-server/tests/it/fhir/summary.rs` (`the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node`); [Laboratory reports in the list](../integrate/laboratory-reports.md#laboratory-reports-in-the-list); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_members_laboratory_composition_is_listed_and_read_as_a_laboratory_report`); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_served_laboratory_report_carries_the_normal_range_and_the_order`) | partial | [#1726](https://github.com/FerroHEALTH/FerroFED/issues/1726), [FerroHEALTH/FerroBRIDGE#1325](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1325), [FerroHEALTH/FerroBRIDGE#1326](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1326), [FerroHEALTH/FerroBRIDGE#1694](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1694), [FerroHEALTH/FerroBRIDGE#1696](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1696), [FerroHEALTH/FerroBRIDGE#1700](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1700), [FerroHEALTH/FerroBRIDGE#1701](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1701) |
| 2.2 | "it shall be able to receive personal electronic health data in the European electronic health record exchange format" | [Written to the declared member](receiving-documents.md#written-to-the-declared-member); [The categories received](information-sheet.md#the-categories-received); `app/ferrofed-server/tests/it/fhir/receive/write.rs` (`a_document_is_written_to_its_member_by_its_ehr_id_alone`); `app/ferrofed-server/tests/it/fhir/receive/refused.rs` (`a_category_no_member_receives_is_refused_naming_it`); [Receiving a dispensation](../integrate/prescriptions.md#receiving-a-dispensation); `app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs` (`a_dispensation_reaches_the_owner_alone_by_its_ehr_id_with_its_body_unchanged`); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_laboratory_report_is_written_to_its_member_by_its_ehr_id_alone`); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_received_normal_range_and_order_reach_the_members_composition`) | partial | [FerroHEALTH/FerroBRIDGE#1325](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1325), [FerroHEALTH/FerroBRIDGE#1326](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1326), [FerroHEALTH/FerroBRIDGE#1694](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1694) |
| 2.3 | "Where an EHR system is designed to provide access to personal electronic health data, it shall be able to receive" | [Written to the declared member](receiving-documents.md#written-to-the-declared-member); [The categories received](information-sheet.md#the-categories-received); `app/ferrofed-server/tests/it/fhir/receive/write.rs` (`a_document_is_written_to_its_member_by_its_ehr_id_alone`); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_laboratory_report_is_written_to_its_member_by_its_ehr_id_alone`); `app/ferrofed-server/tests/it/fhir/receive/laboratory.rs` (`a_received_normal_range_and_order_reach_the_members_composition`) | partial | [FerroHEALTH/FerroBRIDGE#1325](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1325), [FerroHEALTH/FerroBRIDGE#1326](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1326), [FerroHEALTH/FerroBRIDGE#1694](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1694) |
| 2.4 | "shall enable the entry of data with sufficient granularity" | [Intended purpose](regulatory-status.md#intended-purpose); [Art 7(1): transmission without hindrance](claims-review.md#art-71-transmission-without-hindrance) | not-applicable | - |
| 2.5 | "prohibit, restrict or place an undue burden on authorised access, personal electronic health data sharing or use of personal electronic health data for permitted purposes" | [Annex II, point 2.5](claims-review.md#annex-ii-point-25); `app/ferrofed-server/tests/it/completeness.rs` (`partial_where_offered_is_a_200_with_the_answering_nodes_rows`); `app/ferrofed-server/tests/it/overload.rs` (`a_request_past_the_concurrency_limit_is_503_with_retry_after`); `app/ferrofed-server/tests/it/overload.rs` (`without_a_caller_rate_no_caller_is_limited`) | met | - |
| 2.6 | "authorised exporting of personal electronic health data for the reasons of replacing the EHR system by another product" | [What is exported and what is not](../operate/identity-state-export.md#what-is-exported-and-what-is-not); `app/ferrofed-server/tests/it/identity_export.rs` (`the_export_holds_the_learned_creating_system_mappings_in_its_format`); [Reading the log](../operate/access-log.md#reading-the-log); `app/ferrofed-server/tests/it/access/search.rs` (`an_operator_reviews_by_ehr_id_endpoint_caller_and_outcome`) | met | - |

## 3. Requirements for security and logging

| Point | Requirement | Evidence | Status | Issue |
|---|---|---|---|---|
| 3.1 | "shall provide reliable mechanisms for the identification and authentication of health professionals" | [Professionals and assurance](../operate/authentication.md#professionals-and-assurance); `app/ferrofed-server/tests/it/auth/token.rs` (`a_token_from_an_untrusted_issuer_is_401`); `app/ferrofed-server/tests/it/auth/professional.rs` (`a_token_below_the_least_level_is_401_and_reaches_no_node`); `app/ferrofed-server/tests/it/auth/professional.rs` (`a_client_token_naming_a_professional_is_refused_at_any_level`) | met | - |
| 3.2 | "shall provide sufficient logging mechanisms that record at least the following information on every access event or group of events" | [The access log](../operate/access-log.md#the-access-log); `app/ferrofed-server/tests/it/access/routed.rs` (`every_access_whose_record_the_spool_cannot_take_is_refused_with_no_data`); `app/ferrofed-server/tests/it/access/gate.rs` (`a_patient_data_answer_that_reports_no_access_is_withheld`); `app/ferrofed-server/tests/it/access/query.rs` (`a_stored_query_execution_is_recorded_under_its_name`); `app/ferrofed-server/tests/it/fhir/summary.rs` (`a_summary_is_recorded_once_with_the_patient_summary_category`); `app/ferrofed-server/tests/it/fhir/receive/audit.rs` (`a_receipt_is_recorded_once_as_a_creation_of_its_category`) | met | - |
| 3.2(a) | "identification of the healthcare provider or other individuals having accessed the personal electronic health data" | [What a record names](../operate/access-log.md#what-a-record-names); `app/ferrofed-server/tests/it/access/accessor.rs` (`a_person_is_recorded_with_the_professional_and_the_verified_level`) | met | - |
| 3.2(b) | "identification of the specific natural person or persons having accessed the personal electronic health data" | [The person behind an access](../operate/access-log.md#the-person-behind-an-access); `app/ferrofed-server/tests/it/access/accessor.rs` (`a_person_is_recorded_with_the_professional_and_the_verified_level`) | met | - |
| 3.2(c) | "the categories of data accessed" | [Categories](../operate/access-log.md#categories); `app/ferrofed-server/tests/it/access/query.rs` (`a_patient_query_names_the_caller_the_patient_the_origins_and_every_category`); `app/ferrofed-server/tests/it/access/query.rs` (`an_unmapped_template_is_unclassified_with_its_id_and_answered`) | met | - |
| 3.2(d) | "the time and date of access" | [What a record names](../operate/access-log.md#what-a-record-names); `app/ferrofed-server/tests/it/access/retention.rs` (`with_no_retention_table_a_record_is_kept_three_years`); `app/ferrofed-server/tests/it/access/search.rs` (`the_period_bounds_a_search_and_is_required`) | met | - |
| 3.2(e) | "the origin or origins of data" | [What a record names](../operate/access-log.md#what-a-record-names); `app/ferrofed-server/tests/it/access/origins.rs` (`each_origin_of_a_merged_answer_carries_the_categories_of_its_own_rows`) | met | - |
| 3.3 | "shall include tools or mechanisms to review and analyse the log data, or it shall support the connection and use of external software for the same purposes" | [Reading the log](../operate/access-log.md#reading-the-log); `app/ferrofed-server/tests/it/access/search.rs` (`the_persons_search_finds_every_access_to_their_data`); `app/ferrofed-server/tests/it/access/search.rs` (`an_operator_reviews_by_ehr_id_endpoint_caller_and_outcome`); `app/ferrofed-server/tests/it/access/search.rs` (`every_search_of_the_log_is_recorded`) | met | - |
| 3.4 | "shall support different retention periods and access rights that take into account the origins and categories of electronic health data" | [How long a record is kept](../operate/access-log.md#how-long-a-record-is-kept); `app/ferrofed-server/tests/it/access/retention.rs` (`a_category_kept_longer_sets_the_period`); `app/ferrofed-server/tests/it/access/retention.rs` (`an_origin_kept_longer_sets_the_period`) | partial | [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) |

## What is missing

- **1.1:** The risk file leaves the repository's part of hazard L-07 of the logging component, keeping each record for its period and limiting who reads it, at S3, P2 with the deployment, and I-03, I-16, I-18, I-19 and I-20 of the interoperability component open at S4, P3, so that component is not offered for its Annex II purpose.
- **1.3:** Rights of natural persons holds each right of Chapter II to the components. Restriction (Art 8), the information on access (Art 9), the emergency mark (Art 11(5)), transmission without hindrance (Art 7(1)), information a person inserts, marked as theirs (Art 5), and the absence of a fee (Art 18) are met, and a person's own grant gets their summary, and their prescriptions where members serve them (Art 3). A person's own grant lists and reads their laboratory reports (Art 3; fhir/receive/laboratory_grant.rs, a_persons_own_grant_lists_and_reads_their_laboratory_report); imaging and discharge reports have no interface (Art 3); the cross-border route serves the requests a national contact point relays (Art 7(2)); and only patient summaries, laboratory reports and dispensations are received (Art 7(4)).
- **1.4:** The patient summary is validated against the vendored HL7 Europe profiles, but the exchange format of Art 15(1) is not adopted and the European digital testing environment has not been run, so the interoperability of the component cannot yet be shown against either.
- **2.1:** The FHIR face serves the patient summary, laboratory, discharge and imaging reports in the document list with their read, and prescriptions and dispensations from members that serve them in the HL7 Europe MPD profiles, with hazards I-03, I-18, I-19 and I-20 of the interoperability component open.
- **2.2:** A release receives patient summaries and laboratory, imaging and discharge reports, with hazard I-16 open for laboratory reports, and dispensations, written to the member that owns their prescription; prescriptions are served, never received.
- **2.3:** As for 2.2.
- **3.4:** Each record states its retention period by its origins and categories and carries a security label per category and per origin; limiting access by them is the Audit Record Repository's, and the Art 36(1) common specifications, not yet adopted, may add to the point.

## Notes on the met and not-applicable points

- **2.4:** FerroFED has no function for entering data: a write is composed in the client's own application and forwarded to the member that controls the record as the client sent it, so the granularity of entry is that application's.
- **2.5:** Assessed for the features built; the assessment is repeated as the interoperability component grows.
- **2.6:** The clinical data stays at the member CDRs; the gateway exports the routing state it learned, and the access records are exported from the Audit Record Repository.
- **3.1:** The deployment's issuer authenticates the professional; the gateway verifies its token and holds it to the assurance level the deployment sets.
- **3.2(a):** The record's wire form is tested by the ehds-logging test the_person_the_provider_and_the_client_are_named (FerroHEALTH/FerroAUDIT, at its exact pin in the root Cargo.toml).
- **3.2(b):** The record names the professional as far as the issuer's token states it. The record's wire form is tested by the ehds-logging test the_persons_agent_carries_the_professional_the_level_and_who_acts (FerroHEALTH/FerroAUDIT, at its exact pin in the root Cargo.toml).
- **3.2(e):** The record's wire form is tested by the ehds-logging test the_categories_and_the_origins_ride_in_their_own_entities (FerroHEALTH/FerroAUDIT, at its exact pin in the root Cargo.toml).
- **3.3:** Met by the second alternative: the records go to the deployment's Audit Record Repository and are read there with ITI-81.

<!-- annex-ii:end -->
