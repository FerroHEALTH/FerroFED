<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# MyHealth@EU self-assessment

This page is FerroFED's own assessment of the gateway against the
MyHealth@EU Requirements Catalogue **v10.0.0** (W10-Operation Ready,
2026-03-06). It is not a MyHealth@EU compliance check, and FerroFED holds no
Operation Ready status: MyHealth@EU runs its own compliance checks for the
national contact points in operation, and FerroFED is not a national contact
point. The badge in the README counts the rows below and says
self-assessed; it never says compliant, certified or operation ready.

## What the catalogue binds

The catalogue's requirements address the national contact points for
digital health and MyHealth@EU. FerroFED is one of the national
infrastructures a contact point calls through its national connector
([A national contact point](../integrate/national-contact-point.md)), so a
requirement binds the gateway only where the contact point of the country of
affiliation relies on what the national infrastructure answers: the patient
lookup, the patient summary, the prescriptions and the dispensations it
receives, the document categories, the quality of the data, and the record
of every access.

Every row judges the gateway's share of one requirement, and its conclusion
names whose the rest is:

| Status | What it means |
|---|---|
| met | the gateway's share is done, and the evidence shows it |
| partly-met | part of the gateway's share is done; the conclusion names the rest and its issue |
| waits-art-15-1 | the share depends on the exchange format the Art 15(1) implementing act will fix |
| not-met | the gateway does not do its share yet; the issue tracks it where one is planned |
| not-assessed | the repository holds no decision, page or test to judge the share by |
| not-applicable | the requirement binds the contact points, the country of treatment or MyHealth@EU, not the gateway |

The badge's count is the rows met out of every row but the not-applicable
ones. A row whose status the repository's own decisions, pages and tests
cannot show is never marked met.

## The source and its terms

The catalogue is privileged-access: the Xt-EHR deliverable D6.1 (section 5)
says it is available only with authorisation, and Commission Decision
2011/833/EU Art 2(2)(c) excludes such documents from its reuse grant. The
owner read v10.0.0 under an EU Login reading grant, and FerroFED has no grant
to reuse it. This page, `conformance/myhealth-eu.tsv` and the badge therefore
carry requirement ids, section numbers, statuses and FerroFED's own
conclusions, and none of the catalogue's text. `scripts/checks/myhealth-eu.sh`
holds the mapping, this page and the badge in step, and refuses a copy of
the catalogue in the repository.

The implementing act on the exchange format (Regulation (EU) 2025/327
Art 15(1)) is not adopted. When it is, every row is checked again with the
decisions it rests on (#1425).

## The requirements

<!-- myhealth-eu:begin -->
<!-- Rendered from conformance/myhealth-eu.tsv by scripts/conformance/myhealth-eu.sh --write. Do not edit between the markers. -->

Of the 73 rows, 31 bind the gateway: 17 met, 3 partly met, 0 waiting on the Art 15(1) act, 11 not met and 0 not assessed. The other 42 do not apply to the gateway.

| Id | Section | Status | FerroFED's conclusion | Evidence | Issue |
|---|---|---|---|---|---|
| 01 | §1.9 | met | The gateway requires and records the professional and provider attributes the contact point asserts and never refuses on role or point of care; identifying and authenticating the professional is the country of treatment's. | `docs/architecture.md::A69`<br>`docs/architecture.md::A70`<br>`app/ferrofed-server/tests/it/contact_point/admission.rs::a_request_missing_any_annex_attribute_is_403_with_nothing_sent` | - |
| 01.01 | §1.9.1 | not-applicable | The country of treatment identifies and authenticates the professional; the gateway receives the result as the contact point asserts it. | `docs/architecture.md::A69` | - |
| 01.02 | §1.9.2 | met | The gateway never refuses on the professional's role or point of care; mapping the agreed roles to national ones is the contact point's. | `docs/architecture.md::A70`<br>`website/book/src/integrate/national-contact-point.md::What the connector calls` | - |
| 02 | §1.10 | met | The gateway looks a patient up by the identifier the identity services confirmed and answers one patient or none; the identification itself belongs to the identity services and the contact point. | `docs/architecture.md::A68`<br>`app/ferrofed-server/tests/it/fhir/lookup.rs::a_patient_a_member_holds_is_found_as_the_supplier_holds_it_and_no_member_is_asked`<br>`app/ferrofed-server/tests/it/fhir/lookup.rs::an_identifier_no_member_holds_and_a_restricted_patient_answer_alike` | - |
| 02.01 | §1.10.1 | met | Same share as 02: the lookup by a confirmed identifier, never a choice among several patients. | `docs/architecture.md::A68`<br>`app/ferrofed-server/tests/it/fhir/lookup.rs::a_search_by_demographics_or_without_an_identifier_is_refused_naming_identifier`<br>`website/book/src/integrate/patient-summary.md::Looking a patient up` | - |
| 02.01.01 | §1.10.1.1 | not-applicable | Identification by demographics or eID belongs to the identity services and the contact point; the gateway searches no demographics. | `docs/architecture.md::A68`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 02.01.02 | §1.10.1.2 | not-applicable | The national configuration file belongs to the contact point. | `docs/architecture.md::A68`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 02.01.03 | §1.10.1.3 | not-applicable | Identification through the EU digital identity wallet belongs to the contact point. | `docs/architecture.md::A68`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 03 | §1.11 | not-applicable | This is the contact point's own operation and trust between countries; FerroFED keeps its own incident and support process for its users. | `docs/architecture.md::A67`<br>`website/book/src/evaluate/post-market.md` | - |
| 03.01 | §1.11.1 | not-applicable | This is the contact point's own operation. | `docs/architecture.md::A67` | - |
| 03.02 | §1.11.2 | not-applicable | This is the contact point's own operation. | `docs/architecture.md::A67` | - |
| 03.03 | §1.11.3 | not-applicable | This is the contact point's own operation. | `docs/architecture.md::A67` | - |
| 03.04 | §1.11.4 | not-applicable | This is the contact point's own operation. | `docs/architecture.md::A67` | - |
| 04 | §1.12 | not-applicable | Data protection within MyHealth@EU belongs to the contact points as joint controllers; FerroFED records its own processing on its data protection page. | `docs/architecture.md::A67`<br>`website/book/src/evaluate/data-protection.md` | - |
| 04.01 | §1.12.1 | not-applicable | This belongs to the contact points and the MyHealth@EU central services. | `docs/architecture.md::A67`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 04.02 | §1.12.2 | not-applicable | This belongs to the contact points and the MyHealth@EU central services. | `docs/architecture.md::A67`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 04.03 | §1.12.3 | not-applicable | This belongs to the contact points and the MyHealth@EU central services. | `docs/architecture.md::A67`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 04.04 | §1.12.4 | not-applicable | This belongs to the contact points and the MyHealth@EU central services. | `docs/architecture.md::A67`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 05 | §1.13 | met | The gateway serves the patient summary from every member that holds the patient on the FHIR face the contact point calls. | `docs/architecture.md::A66`<br>`docs/architecture.md::A67`<br>`website/book/src/integrate/patient-summary.md::Asking for a summary`<br>`app/ferrofed-server/tests/it/e2e/national/served.rs::the_national_connector_reads_the_summary_and_prescriptions_and_dispenses` | - |
| 05.01 | §1.13.1 | met | The gateway builds the document with its sections, empty reasons, nature and author as decided in A65. | `docs/architecture.md::A65`<br>`website/book/src/integrate/patient-summary.md::The document`<br>`app/ferrofed-server/tests/it/fhir/summary.rs::an_imported_copy_is_listed_once_from_the_member_that_created_it` | - |
| 05.02 | §1.13.2 | met | The gateway writes the original information as the narrative of every section, from which the contact point renders the original; transcoding, translation and the cross-border exchange are the contact points'. | `docs/architecture.md::A72`<br>`website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 05.03 | §1.13.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 06 | §1.14 | partly-met | The gateway serves the prescriptions each member serves at its own endpoint; prescriptions mapped from openEHR medication data wait on #1403, and which prescriptions are available abroad is each member's selection, which the gateway does not check. | `docs/architecture.md::A67`<br>`website/book/src/integrate/prescriptions.md::Searching a patient's prescriptions`<br>`app/ferrofed-server/tests/it/e2e/national/served.rs::the_national_connector_reads_the_summary_and_prescriptions_and_dispenses` | #1403 |
| 06.01 | §1.14.1 | partly-met | A member's prescription passes through unchanged, so its content and missing-data reasons are the member's; content mapped from openEHR waits on #1403. | `website/book/src/integrate/prescriptions.md::The member contract`<br>`app/ferrofed-server/tests/it/fhir/prescriptions/mod.rs::each_members_prescriptions_are_answered_as_sent_with_a_provenance_naming_it` | #1403 |
| 06.02 | §1.14.2 | partly-met | The gateway serves the list by search; the original-language source of the PDF is each member's own resource narrative, passed through unchecked; transcoding and translation are the contact points'. | `website/book/src/integrate/prescriptions.md::Searching a patient's prescriptions`<br>`docs/architecture.md::A72` | - |
| 06.03 | §1.14.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 07 | §1.15 | met | The gateway writes a dispensation to the member that owns the prescription and answers with its creation or relays its refusal; substitution belongs to the country of treatment and the owner. | `website/book/src/integrate/prescriptions.md::Receiving a dispensation`<br>`app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::a_dispensation_reaches_the_owner_alone_by_its_ehr_id_with_its_body_unchanged`<br>`app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::the_owners_refusal_is_a_conflict_naming_it`<br>`app/ferrofed-server/tests/it/e2e/national/served.rs::the_national_connector_reads_the_summary_and_prescriptions_and_dispenses` | - |
| 07.01 | §1.15.1 | met | The gateway checks a dispensation against its profile and requires its identifier, which the owner stores; its content is the country of treatment's. | `app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::a_dispensation_that_breaks_its_profile_asks_no_member`<br>`app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::a_dispensation_with_no_business_identifier_is_refused_naming_the_element` | - |
| 07.02 | §1.15.2 | not-applicable | Transcoding the dispensation belongs to the contact point of the country of treatment. | `docs/architecture.md::A72` | - |
| 07.03 | §1.15.3 | met | The gateway confirms only after the owner created the dispensation and never sends it twice; updating the available prescriptions is the owner's. | `app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::an_owner_that_does_not_answer_is_asked_once_and_the_outcome_is_unknown`<br>`website/book/src/integrate/prescriptions.md::The member contract` | - |
| 07.04 | §1.15.4 | not-met | The face offers no working discard: a withdrawal sent again under the same identifier is refused as a duplicate (#1450). | `website/book/src/integrate/prescriptions.md::Receiving a dispensation` | #1450 |
| 08 | §1.16 | not-met | The face serves no laboratory, discharge or imaging document yet. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1404 #1405 #1406 |
| 08.01 | §1.16.1 | not-met | As 08. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1404 #1405 #1406 |
| 08.02 | §1.16.2 | not-applicable | The cross-border exchange belongs to the contact points. | `docs/architecture.md::A67` | - |
| 08.03 | §1.16.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 09 | §1.17 | not-applicable | Patient access is the Member State's access service, which FerroFED is not. | `website/book/src/evaluate/natural-person-rights.md::Summary` | - |
| 09.01 | §1.17.1 | not-applicable | Patient access is the Member State's access service, which FerroFED is not. | `website/book/src/evaluate/natural-person-rights.md::Summary` | - |
| 09.01.01 | §1.17.1.1 | not-applicable | Patient access is the Member State's access service, which FerroFED is not. | `website/book/src/evaluate/natural-person-rights.md::Summary` | - |
| 09.02 | §1.17.2 | not-applicable | Patient access is the Member State's access service, which FerroFED is not. | `website/book/src/evaluate/natural-person-rights.md::Summary` | - |
| 10 | §1.18 | met | The gateway exchanges sectioned, coded data with codes as recorded, an equivalent coding only for an exact mapping, and a narrative in the original language. | `docs/architecture.md::A63`<br>`docs/architecture.md::A65`<br>`website/book/src/integrate/patient-summary.md::Terminology` | - |
| 10.01 | §1.18.1 | met | The gateway's share is the sectioned document with codes as recorded; transcoding to the Master Value Sets Catalogue is the contact point's. | `docs/architecture.md::A63`<br>`docs/architecture.md::A72`<br>`website/book/src/integrate/patient-summary.md::The document` | - |
| 10.02 | §1.18.2 | met | The gateway adds an equivalent coding only for an exact term mapping and puts any other match in the narrative. | `docs/architecture.md::A63` | - |
| 10.03 | §1.18.3 | met | The gateway lists an imported copy once and keeps two compositions of one fact. | `docs/architecture.md::A65`<br>`app/ferrofed-server/tests/it/fhir/summary.rs::an_imported_copy_is_listed_once_from_the_member_that_created_it`<br>`app/ferrofed-server/tests/it/fhir/summary.rs::two_different_compositions_are_both_listed` | - |
| 11 | §1.19 | not-applicable | Security, performance, traceability and auditability of the cross-border services are the contact points'; the gateway's share is under 11.03 and 11.04. | `docs/architecture.md::A71` | - |
| 11.01 | §1.19.1 | not-applicable | This is the contact point's security management; FerroFED documents its own hardening. | `website/book/src/operate/hardening.md` | - |
| 11.01.01 | §1.19.1.1 | not-applicable | This belongs to the contact point; the gateway authenticates every caller and serves over TLS. | `website/book/src/operate/hardening.md::TLS`<br>`website/book/src/operate/hardening.md::Callers and issuers` | - |
| 11.01.02 | §1.19.1.2 | not-applicable | End-to-end integrity runs from the contact point onward; the gateway passes a member's resource unchanged. | `website/book/src/integrate/prescriptions.md::Searching a patient's prescriptions` | - |
| 11.01.03 | §1.19.1.3 | not-applicable | This belongs to the contact point; the gateway's health and readiness routes feed its monitoring. | `website/book/src/operate/health.md::The routes` | - |
| 11.01.04 | §1.19.1.4 | not-applicable | Anomaly detection belongs to the contact point; the access record and its correlation identifier feed it. | `docs/architecture.md::A71`<br>`website/book/src/operate/access-log.md::A request a national contact point relays` | - |
| 11.01.04.01 | §1.19.1.4.1 | not-applicable | Anomaly detection belongs to the contact point; the access record and its correlation identifier feed it. | `docs/architecture.md::A71`<br>`website/book/src/operate/access-log.md::A request a national contact point relays` | - |
| 11.01.04.02 | §1.19.1.4.2 | not-applicable | Anomaly detection belongs to the contact point; the access record and its correlation identifier feed it. | `docs/architecture.md::A71`<br>`website/book/src/operate/access-log.md::A request a national contact point relays` | - |
| 11.02 | §1.19.2 | not-applicable | Performance monitoring belongs to the contact point; the gateway's metrics feed it. | `website/book/src/operate/metrics.md::The metrics` | - |
| 11.03 | §1.19.3 | met | The gateway's access record names the foreign professional and provider, the time and the data, with the correlation identifier that joins it to the contact point's log. | `docs/architecture.md::A71`<br>`website/book/src/operate/access-log.md::A request a national contact point relays`<br>`app/ferrofed-server/tests/it/access/relayed.rs::the_record_carries_the_correlation_identifier` | - |
| 11.04 | §1.19.4 | met | The same record, with the emergency mark, kept and read as A75 decides. | `docs/architecture.md::A71`<br>`docs/architecture.md::A75`<br>`website/book/src/operate/access-log.md::Emergency access`<br>`app/ferrofed-server/tests/it/access/accessor.rs::a_contact_point_caller_names_the_foreign_professional_and_provider` | - |
| 12 | §1.20 | not-applicable | Non-repudiation is between the contact points; the record's version uids show what the gateway served and wrote. | `docs/architecture.md::A71` | - |
| 12.01 | §1.20.1 | not-applicable | As 12. | `docs/architecture.md::A71` | - |
| 13 | §1.21 | not-met | The face serves and receives no document of this category yet. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1404 |
| 13.01 | §1.21.1 | not-met | As 13. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1404 |
| 13.02 | §1.21.2 | not-applicable | Transcoding, translation and exchange are the contact points'; the format FerroFED serves is decided in A62. | `docs/architecture.md::A62`<br>`docs/architecture.md::A72` | - |
| 13.03 | §1.21.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 14 | §1.22 | not-met | The face serves and receives no document of this category yet. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1405 |
| 14.01 | §1.22.1 | not-met | As 14. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1405 |
| 14.02 | §1.22.2 | not-applicable | Transcoding, translation and exchange are the contact points'; the format FerroFED serves is decided in A62. | `docs/architecture.md::A62`<br>`docs/architecture.md::A72` | - |
| 14.03 | §1.22.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 15 | §1.23 | not-met | The face serves and receives no document of this category yet. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1406 |
| 15.01 | §1.23.1 | not-met | As 15. | `website/book/src/evaluate/information-sheet.md::The categories received` | #1406 |
| 15.02 | §1.23.2 | not-applicable | Transcoding, translation and exchange are the contact points'; the format FerroFED serves is decided in A62. | `docs/architecture.md::A62`<br>`docs/architecture.md::A72` | - |
| 15.03 | §1.23.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 16 | §1.24 | not-met | The face serves no vaccination history document; it is outside the Art 14(1) categories FerroFED builds. | `website/book/src/evaluate/information-sheet.md::The categories received` | - |
| 16.01 | §1.24.1 | not-met | As 16. | `website/book/src/evaluate/information-sheet.md::The categories received` | - |
| 16.02 | §1.24.2 | not-applicable | Transcoding, translation and exchange are the contact points'. | `docs/architecture.md::A72` | - |
| 16.03 | §1.24.3 | not-applicable | Display to the professional is the country of treatment's. | `website/book/src/integrate/national-contact-point.md::What stays with the contact point` | - |
| 1.25 | §1.25 | met | Every failure of the face tells a temporary failure from a permanent one and relays a member's refusal after the gateway's issue. | `website/book/src/integrate/national-contact-point.md::Failures the connector relays`<br>`app/ferrofed-server/tests/it/fhir/prescriptions/dispense.rs::the_owners_own_issues_follow_the_gateways_on_a_refusal_and_an_error`<br>`app/ferrofed-server/tests/it/contact_point/admission.rs::the_refusal_names_each_missing_attribute_and_no_value` | - |

<!-- myhealth-eu:end -->
