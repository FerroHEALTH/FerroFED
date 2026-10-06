<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Clinical safety risk file

Annex II, point 1.1, of Regulation (EU) 2025/327 on the European Health Data
Space (the EHDS Regulation) asks that the harmonised software components of
an EHR system "achieve the performance intended by its manufacturer" and be
designed and manufactured so that, "during normal conditions of use, they
are suitable for their intended purpose and their use does not put at risk
patient safety". This page is FerroFED's risk management file for its two
harmonised components (Art 25(1)): the European logging software component
and the European interoperability software component. It names each
hazard, the control that answers it, and the test or code that implements
the control, by path. A control that is not built is marked planned, with
its issue.

The Annex II checklist that
[#525](https://github.com/FerroHEALTH/FerroFED/issues/525) builds cites this
page for point 1.1, by hazard id. Every quotation of the Regulation is from
the Official Journal text, vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice.

## Scope

| Component | Where it is | What it does in this release |
|---|---|---|
| Logging (Art 2(2)(o)) | `crates/ehds-logging`, with the record built in `app/ferrofed-server/src/access/` | records every access to patient data the gateway intermediates ([The access log](../operate/audit.md#the-access-log)) |
| Interoperability (Art 2(2)(n)) | `crates/eehrxf` | a library: the EHDS dataset model and the mapping of one openEHR composition to a FHIR R4 `Bundle`; the gateway does not serve it |

Both components ride on the federated query path: the logging component
records what that path did, and the interoperability component will build
its documents from what that path returns. The federation hazards the
interoperability component inherits are therefore assessed here with it,
and their controls on the query path are named.

The file covers the use the [intended purpose](regulatory-status.md#intended-purpose)
states and the misuse a deployment can reasonably make of it: a wrong
configuration, a client that ignores `meta.federation`, a member that
breaks the conditions of membership. It does not cover a client
application's own presentation, which is that application's to assess.

## Method

The Regulation names no standard for this assessment, so no specification
governs the method: our own design. The file follows the risk management
process of EN ISO 14971:2019, *Medical devices: Application of risk
management to medical devices*, as a method. FerroFED claims no conformity
with that standard, and its documents do not classify it as a medical
device ([The medical device question](data-protection.md#the-medical-device-question)).
The standard's text is copyrighted and not vendored; the clauses below are
named by the standard's table of contents.

| EN ISO 14971:2019 | Where this file answers it |
|---|---|
| 4.4 Risk management plan, 4.5 Risk management file | this page, [Method](#method) and [Review and change](#review-and-change) |
| 5.2 Intended use and reasonably foreseeable misuse | [Scope](#scope) |
| 5.4 Identification of hazards and hazardous situations | the hazard tables |
| 5.5 Risk estimation | the severity and occurrence scales below, the "Before" column |
| 6 Risk evaluation | the acceptability rule below |
| 7.1 Risk control option analysis | the order of controls below |
| 7.2 Implementation of risk control measures | the "Control" and "Implemented by" columns |
| 7.3 Residual risk evaluation | the "After" column |
| 7.5 Risks arising from risk control measures | the hazards a control itself creates, L-02 and L-05 |
| 8 Evaluation of overall residual risk | [Overall residual risk](#overall-residual-risk) |
| 9 Risk management review, 10 Production and post-production activities | [Review and change](#review-and-change) |

**Sources of hazards.** The four hazards a federated summary adds to a CDR's
own, named when this file was opened: duplicate entries across nodes, a
silent node, an unmapped code and a demographic mismatch. Then the Federation
Tier's failure statuses (§11.1, §11.2), the serious-incident classes of
[Complaints and incidents](post-market.md#serious-incidents), the
[threat model](threat-model.md), and each point of Annex II, 3.2 to 3.4, read
as "what if this field is wrong or missing".

**Severity.** The harm the hazardous situation can lead to. Art 2(2)(r)
names the serious ones: "the death of a natural person or serious harm to a
natural person's health" and "serious prejudice to a natural person's
rights".

| Level | Harm |
|---|---|
| S1 | none to care or rights |
| S2 | care is delayed, or a record of access is incomplete in a way the deployment can see and correct |
| S3 | a person's rights are prejudiced: a restriction is shown, an access goes unrecorded, a patient identifier is disclosed; or a clinician decides on information that is incomplete or wrong where the harm is reversible |
| S4 | a clinician decides on information that is incomplete or wrong, where the harm to health can be serious |

**Occurrence.** For software, how readily the hazardous situation arises,
judged qualitatively.

| Level | The hazardous situation arises |
|---|---|
| P1 | only through two independent faults, or a misconfiguration `config check` refuses |
| P2 | through one fault in another system, or one misconfiguration nothing refuses |
| P3 | in normal use |

**Acceptability.** A risk is acceptable after control at S1 at any level, at
S2 at P2 or P1, and at S3 or S4 at P1 alone. A control counts only when it is
built and a test holds it. A hazard whose control is planned keeps its
"Before" rating and is open; the component it belongs to is not offered for
its Annex II purpose while a hazard of S3 or S4 is open.

**The order of controls**, after clause 7.1: a design that cannot produce the
hazardous situation first; then a protective measure in the software, such
as refusing the request or failing closed; then information for safety,
which the [instructions for use](../operate/instructions-for-use.md#reading-an-answer-safely)
carry.

## The logging component

| Id | Hazardous situation | Harm | Before | Control | Implemented by | After |
|---|---|---|---|---|---|---|
| L-01 | An access to patient data leaves no access record: the sink cannot store it, or a path of the gateway hands out data without one | the person cannot learn who read their data (Art 9), and misuse goes unseen | S3, P2 | the record is stored before the answer leaves; when it cannot be, the access is refused `503 access-unrecorded` with no data; an answer that carries data and reports no access is withheld; only the server builds a record | `app/ferrofed-server/src/access/mod.rs`; `app/ferrofed-server/tests/it/access/routed.rs` (`every_access_whose_record_the_spool_cannot_take_is_refused_with_no_data`, `a_stored_query_whose_record_the_spool_cannot_take_is_refused_with_no_data`); `app/ferrofed-server/tests/it/access/gate.rs` (`a_patient_data_answer_that_reports_no_access_is_withheld`); `app/ferrofed-engine/tests/it/architecture.rs` (`no_crate_but_the_server_builds_an_access_record`) | S3, P1 |
| L-02 | Failing closed blocks care: a full spool refuses every access to patient data (a risk the L-01 control creates) | a clinician gets no answer, care is delayed | S2, P2 | the spool is bounded on disk, so a repository outage alone blocks nothing; alerts fire on the backlog and on refusals; the instructions name the alert | `app/ferrofed-server/tests/it/audit_repository.rs` (`a_full_spool_fails_the_query_closed_and_asks_no_member`); `app/ferrofed-server/tests/it/feed_audit/pixm.rs` (`a_repository_that_never_answers_holds_no_query_and_the_records_are_spooled`); `deploy/observability/ferrofed-alerts.yaml` (`FerroFEDAuditSpoolBacklog`, `FerroFEDAuditRefused`) | S2, P1 |
| L-03 | The record names the wrong person or provider | an access is attributed to someone who did not make it, and the one who did is not found | S3, P2 | the accessor comes from the token the gateway verified, never from the request; a request refused at authentication, for its person or assurance, or by a limit, writes no record; a console query names the operator | `app/ferrofed-server/tests/it/access/query.rs` (`a_patient_query_names_the_caller_the_patient_the_origins_and_every_category`, `a_console_query_names_the_operator`); `app/ferrofed-server/tests/it/access/professional.rs`; `app/ferrofed-server/tests/it/access/limits.rs`; `crates/ehds-logging/tests/it/balp.rs` (`the_person_the_provider_and_the_client_are_named`) | S3, P1 |
| L-04 | The record omits the professional's identification and assurance level, or the client's address behind a proxy | a reviewer cannot tell which professional acted, or from where | S2, P3 | planned: [#742](https://github.com/FerroHEALTH/FerroFED/issues/742), [#722](https://github.com/FerroHEALTH/FerroFED/issues/722) | | open, S2, P3 |
| L-05 | An access to priority-category data is recorded as of no category, so it is kept for less time and the person's view of it is wrong; or the map sends every access to "unclassified" (a risk the control creates) | the access is lost to review before its time | S3, P2 | `none` only when every object is an exact `none` key; everything else `ehds-unclassified` with its ids as evidence, never refused; a map with an unknown code is refused at load; the map's digest is in every record; the deployment authors the map | `crates/ehds-logging/tests/it/classify.rs` (`an_unmapped_object_is_unclassified_with_its_ids_and_never_dropped`, `none_never_erases_a_category_in_a_mixed_result`, `an_id_differing_by_case_space_or_specialisation_is_unmapped_never_none`); `crates/ehds-logging/tests/it/property.rs` (`a_result_is_of_no_category_only_when_every_object_is_an_exact_none`, `evidence_that_shows_nothing_is_unclassified`); `crates/ehds-logging/tests/it/map.rs` (`a_code_that_names_no_category_is_refused`) | S3, P1 for "no category"; unclassified records are kept, retention by category is L-07 |
| L-06 | The record names an origin that sent nothing, an `ehr_id` at a member the query never reached, or the categories of one member's rows under another | the record misstates which CDRs released which data | S2, P3 | an endpoint is an origin only when the query was sent to it, through the endpoint it was sent through; a query sent to no member writes no record; each origin of a merged answer carries the categories of its own rows; a member past the answer bound is `node-error` with no row | `app/ferrofed-server/tests/it/access/origins.rs` (`a_query_every_member_of_which_was_capped_writes_no_record`, `a_capped_member_is_no_origin_of_the_answer`, `each_origin_of_a_merged_answer_carries_the_categories_of_its_own_rows`); `app/ferrofed-server/tests/it/access/query.rs` (`a_query_no_node_was_sent_is_not_an_access`, `a_member_answer_past_the_read_bound_is_recorded_as_node_error_with_no_row`) | S2, P1 |
| L-07 | The records cannot be reviewed, or are not kept by origin and category (Annex II, points 3.3 and 3.4) | misuse is not found, or records go before their time | S3, P2 | the records go to the deployment's Audit Record Repository, external software point 3.3 admits; the gateway's own review, retention and export are planned ([#660](https://github.com/FerroHEALTH/FerroFED/issues/660), [#521](https://github.com/FerroHEALTH/FerroFED/issues/521)) | `[audit] destination = "repository"`, refused unset or `off` outside development: `app/ferrofed-server/tests/it/feed_audit/config.rs` (`a_binding_outside_development_needs_a_declared_destination`, `off_is_refused_outside_development_and_admitted_inside`) | open, S3, P2 |
| L-08 | The record discloses the patient or the caller where it must not: to a node, the operator log, a span or a metric | a patient identifier leaks (§5.4, N33) | S3, P2 | no record content reaches a node or a log; `Debug` shows no identifier | `app/ferrofed-server/tests/it/access/query.rs` (`no_patient_template_or_caller_reaches_the_log_a_metric_or_a_node`); `crates/ehds-logging/tests/it/balp.rs` (`no_debug_shows_the_patient_the_person_or_an_id`); `crates/ehds-logging/tests/it/classify.rs` (`no_debug_shows_an_id`) | S3, P1 |
| L-09 | Emergency access to restricted data is not marked | the person cannot see that a restriction was overridden (Art 8) | S3, P2 | a record whose verified token declares a purpose of use the deployment names in `[[access_log.emergency_purpose]]` carries the `ehds-emergency-access` entity, stated in words, with the purposes that marked it; the mark is read from the token alone, never inferred, and changes no dispatch and no answer, so the node still decides (N26); `config check` notes a deployment that names no emergency purpose, and the instructions for use ask for one; whether restricted data were released is the node's to record | `app/ferrofed-server/tests/it/access/emergency.rs` (`a_declared_emergency_purpose_marks_the_record`, `no_access_is_marked_without_a_declared_purpose`, `a_token_without_the_purpose_is_not_marked`, `a_nodes_refusal_of_an_emergency_access_stands`, `config_check_notes_an_access_log_without_an_emergency_purpose`); `crates/ehds-logging/tests/it/emergency.rs` (`only_an_exact_match_marks_the_access`); `crates/ehds-logging/tests/it/balp.rs` (`an_emergency_access_is_marked_in_its_own_entity`) | S3, P1 |

## The interoperability component

The gateway does not serve this component in this release, so none of the
situations below can arise in use today. Each is open until its control is
built, and the component is not offered for its Annex II purpose while any
of S3 or S4 is open.

| Id | Hazardous situation | Harm | Before | Control | Implemented by | After |
|---|---|---|---|---|---|---|
| I-01 | A silent node: a member that holds the patient's data does not answer, and the summary reads as the whole record | a missed allergy or medication leads to a wrong treatment | S4, P3 | all-or-nothing by default (N37), a partial document only on request and with every silent member named; a section says "no information" (`emptyReason` `nilknown`) only when every member in scope answered empty, `unavailable` otherwise | built on the query path: `app/ferrofed-server/tests/it/completeness.rs` (`all_stated_explicitly_is_accepted_and_fails_closed`, `incompleteness_is_the_complete_flag_and_never_an_operation_outcome`), `app/ferrofed-server/tests/it/endpoint_report.rs` (`an_undirected_query_asks_every_member_and_excludes_none`); the section queries run on that path, every member asked and named: `app/ferrofed-server/tests/it/stored/reserved.rs` (`each_section_query_runs_by_name_and_no_patient_identifier_reaches_a_node`); the document: `app/ferrofed-server/tests/it/fhir/summary.rs` (`a_silent_member_fails_the_summary_and_is_named`, `under_partial_a_silent_member_is_named_in_every_section`), `app/ferrofed-eehrxf/tests/it/summary.rs` (`a_silent_member_is_named_and_its_sections_are_unavailable`) | open, S4, P3 |
| I-02 | Duplicate entries across nodes: two members hold the same medication or problem, and the summary lists it twice, or merges two different entries as one | a dose counted twice, or a distinct entry lost | S4, P3 | entries are never merged across members; each is listed with its provenance, one `Provenance` per source composition and each section's contributing member as its author; version-identity de-duplication on the query path removes only copies of one version; the instructions tell clinicians to expect duplicates | `crates/eehrxf/tests/it/mapping.rs` (`a_composition_maps_to_its_resource_with_one_provenance`); `app/ferrofed-server/tests/it/dedup.rs` (`without_the_header_both_copies_come_back_and_none_is_recorded`, `under_version_identity_one_row_comes_back_and_the_copy_is_named`); [Reading an answer safely](../operate/instructions-for-use.md#reading-an-answer-safely); listed apart, each member a section author with its own `Provenance`: `app/ferrofed-server/tests/it/fhir/summary.rs` (`the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node`), `crates/eehrxf/tests/it/document.rs` (`two_runs_over_one_composition_never_share_a_full_url`) | open, S4, P2 |
| I-03 | An unmapped code or element: content the mapping does not cover is dropped, so an entry reaches the document without its code or not at all | an allergy or a result is missing from the summary | S4, P3 | a composition of another template, a flat composition and a context the mapping files do not declare are refused; unmapped content is reported, never dropped; every producer element of the dataset is covered by the section catalogue; every emitted document is validated against the vendored profiles | `crates/eehrxf/tests/it/mapping.rs` (`a_composition_of_another_template_is_refused`, `a_flat_composition_is_refused`, `a_context_the_files_do_not_declare_is_refused_with_diagnostics`); `crates/eehrxf/tests/it/category.rs` (`the_patient_summary_producer_populates_its_five_required_sections`); the section catalogue: `crates/eehrxf/tests/it/crosswalk.rs` (`the_patient_summary_crosswalk_holds_against_the_pinned_packages`, `a_producer_shall_element_without_a_row_fails`, `an_uncovered_required_section_slice_fails`), with the medical alert (A.2.1.2), a producer element, and the functional status (A.2.3.4) open because no openEHR content is named to feed them ([the patient summary crosswalk](regulatory-status.md#the-patient-summary-crosswalk)); the openEHR content each section is selected by, held to the vendored template: `app/ferrofed-eehrxf/tests/it/template.rs` (`each_section_selects_exactly_the_entry_archetypes_its_template_sections_name`), `app/ferrofed-eehrxf/tests/it/crosswalk.rs` (`every_crosswalk_section_has_a_query_or_a_reason_and_never_both`) ([the section queries](#the-section-queries)); what no mapping covers is counted in the section, never dropped: `app/ferrofed-eehrxf/tests/it/summary.rs` (`a_composition_no_mapping_covers_is_reported_never_dropped`); every document held to the vendored EPS profiles in the tests: `app/ferrofed-server/tests/it/fhir/summary.rs` (`the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node`); planned: [#687](https://github.com/FerroHEALTH/FerroFED/issues/687), [#688](https://github.com/FerroHEALTH/FerroFED/issues/688) | open, S4, P3 |
| I-04 | A code translated to one that is not equivalent: a broader or narrower code presented as the same | a clinician reads a more or less specific diagnosis than was recorded | S3, P2 | the original code is kept; only an RM `TERM_MAPPING` with `match '='`, or a terminology server answer of `equivalent` or `equal`, adds a coding; without a terminology server the component fails closed where one is required | planned: [#687](https://github.com/FerroHEALTH/FerroFED/issues/687) | open, S3, P2 |
| I-05 | A demographic mismatch: the summary carries another patient's data, or a header naming another patient | a decision for one patient made on another's record | S4, P2 | the patient is resolved by identifier through the identity binding and never matched by demographics in the gateway; an ambiguous match is never chosen; an `ehr_id` two members claim is refused and raised as an incident; a PMIR merge drops the bindings it makes stale; the header comes from the identity binding | `app/ferrofed-server/tests/it/pdqm/flow.rs` (`several_matched_patients_refuse_the_resolution_and_fail_the_query`); `app/ferrofed-identity/tests/it/session.rs` (`two_members_with_one_ehr_id_are_ambiguous_and_never_chosen_between`); `app/ferrofed-server/tests/it/ehr_id_collision.rs` (`a_probe_collision_is_a_409_naming_both_with_one_incident_and_neither_is_read`); `app/ferrofed-server/tests/it/pmir/feed.rs` (`an_authenticated_merge_drops_the_bindings_of_the_merged_ehr_ids`); header planned: [#663](https://github.com/FerroHEALTH/FerroFED/issues/663) | open, S4, P2; a wrong link at the identity service stays the deployment's ([threat model](threat-model.md#risks-the-deployment-carries), risk 7) |
| I-06 | A restriction shows through the document: a section marked withheld, or a member's absence that only a restriction explains | the patient's restriction is visible to the provider (Art 8) | S3, P2 | `emptyReason` is never `withheld` while `disclose = false`; a node's refusal and a pre-filter exclusion read as a member without the patient; a national contact point's requests are always served with `disclose = false` | built on the query path: `app/ferrofed-server/tests/it/consent_withheld/mod.rs` (`a_withheld_exclusion_reads_exactly_as_a_member_that_does_not_know_the_patient`), `app/ferrofed-server/tests/it/consent_withheld/node.rs` (`a_node_refusal_reads_exactly_as_a_member_that_does_not_know_the_patient`); the document has no `withheld` empty reason to write (`crates/eehrxf/src/document.rs`, `EmptyReason`); planned for the contact point: [#734](https://github.com/FerroHEALTH/FerroFED/issues/734) | open, S3, P2 |
| I-07 | A section query carries the patient identifier to a node | a patient identifier leaks (§5.4, N33) | S3, P2 | each section is a stored query run through the rewrite and the outbound gate, so a node is asked by its `ehr_id` alone | built on the query path: `app/ferrofed-server/tests/it/outbound.rs` (`either_carrier_and_a_projection_reach_a_node_as_its_ehr_id_alone`), `app/ferrofed-server/tests/it/hygiene.rs`; the section queries ([#776](https://github.com/FerroHEALTH/FerroFED/issues/776)), each held with the patient a parameter and run by name, reach every member with no identifier in the query, path or headers: `app/ferrofed-server/tests/it/stored/reserved.rs` (`each_section_query_runs_by_name_and_no_patient_identifier_reaches_a_node`), `app/ferrofed-eehrxf/tests/it/query.rs` (`the_patient_and_its_namespace_are_parameters_and_nothing_else_is_compared`) | S3, P1 |
| I-08 | The mapping gives different output for the same input, or reads a dataset model other than the pinned one | two requests for one patient disagree, or a profile element is mapped against the wrong definition | S3, P2 | the mapping is deterministic and the dataset model reads the same whatever the order of the package; the package is the pinned one and a malformed one is refused | `crates/eehrxf/tests/it/mapping.rs` (`the_same_composition_maps_to_the_same_bundle`); `crates/eehrxf/tests/it/property.rs` (`the_model_is_the_same_whatever_the_member_order`); `crates/eehrxf/tests/it/dataset.rs` (`the_package_is_the_pinned_one`); `crates/eehrxf/tests/it/refusals.rs` | S3, P1 |
| I-09 | A generated summary is read as one a clinician wrote and attested, or as exhaustive | a clinician trusts it as a complete, attested record | S3, P3 | the author is the gateway's `Device` and the operator's `Organization`, with no attester, and every section's narrative says it is not exhaustive | `app/ferrofed-eehrxf/tests/it/summary.rs` (`a_document_from_two_members_meets_the_eps_profiles`) | open, S3, P3 |
| I-10 | A received document is written to the wrong member or under the wrong patient | a record is filed in another patient's EHR | S4, P2 | a received document goes to one member declared per category, and the answer names it | planned: [#665](https://github.com/FerroHEALTH/FerroFED/issues/665) | open, S4, P2 |
| I-11 | A received document is stored with content lost: the mapping carries only part of it, or a document that breaks its profiles is taken in | a later reader misses an allergy or a medication the sender recorded | S4, P3 | the document is decoded against FHIR R4 and held to the R4 document rules, then checked against its category's profiles with every finding refused and every constraint the check cannot evaluate listed; the original document is kept inline in the composition's `FEEDER_AUDIT.original_content`, and the losses a run declares travel beside the composition; invariants, bindings and the entries' own profiles are planned ([#808](https://github.com/FerroHEALTH/FerroFED/issues/808)) | `crates/eehrxf/tests/it/receive/read.rs` (`a_property_r4_does_not_define_is_refused_with_its_path`, `a_document_whose_first_entry_is_no_composition_is_refused`); `crates/eehrxf/tests/it/receive/conform.rs` (`a_missing_required_section_is_found`, `a_closed_slicing_finds_every_section_no_slice_admits`, `a_pattern_form_the_model_does_not_read_is_listed_as_not_evaluated`); `crates/eehrxf/tests/it/receive/openehr.rs` (`a_document_maps_to_one_composition_that_keeps_the_original`); writing it to a member is planned ([#802](https://github.com/FerroHEALTH/FerroFED/issues/802)) | open, S4, P3 |

### The section queries

Each patient summary section is fed by a stored query the gateway holds
read-only, `eu.ferrofed.eehrxf::patient-summary-{section}` at version
`1.0.0`, which selects every whole composition that contains one of the
section's archetypes, with its template id, from every member
([the gateway's own queries](../integrate/stored-queries.md#the-gateways-own-queries)).
The archetypes are those the openEHR International Patient Summary template
of the openEHR international CKM (cid `1013.26.376`, asset version 1, DRAFT)
names in the template section that carries the patient summary section. The
selection waits on this file's clinical review.

| Section (Xt-EHR) | eHN | Template section | Archetypes |
|---|---|---|---|
| `allergiesAndIntolerances` | A.2.1.1 | Allergies & Intolerances | `EVALUATION.adverse_reaction_risk.v1`, `EVALUATION.exclusion_global.v1`, `EVALUATION.absence.v2` |
| `problems` | A.2.3.1 | Problem List, Past History of Illnesses | `EVALUATION.problem_diagnosis.v1`, `EVALUATION.exclusion_global.v1`, `EVALUATION.absence.v2` |
| `medicationSummary` | A.2.4 | Medication Summary | `ACTION.medication.v1`, `EVALUATION.exclusion_global.v1`, `EVALUATION.absence.v2` |
| `medicalDevicesAndImplants` | A.2.3.2 | Medical Devices | `EVALUATION.device_summary.v0` |
| `procedures` | A.2.3.3 | History of Procedures | `ACTION.procedure.v1`, `EVALUATION.absence.v2`, `EVALUATION.exclusion_global.v1` |
| `immunisations` | A.2.2.1 | Immunizations | `ACTION.medication.v1`, `EVALUATION.absence.v2` |
| `socialHistory` | A.2.5 | Social History | `EVALUATION.tobacco_smoking_summary.v1`, `EVALUATION.alcohol_consumption_summary.v1` |
| `pregnancyHistory` | A.2.6 | Pregnancy | `EVALUATION.pregnancy_summary.v0`, `EVALUATION.estimated_date_delivery.v0`, `OBSERVATION.exclusion_pregnancy.v0` |
| `advanceDirectives` | A.2.7.2 | Advanced Directives | `EVALUATION.advance_care_directive.v1`, `EVALUATION.limitation_of_treatment.v0` |
| `observationResults` | A.2.8 | Diagnostic Results, Vital Signs | `OBSERVATION.laboratory_test_result.v1`, `OBSERVATION.imaging_exam_result.v0`, `OBSERVATION.body_weight.v2`, `OBSERVATION.height.v2`, `OBSERVATION.respiration.v2`, `OBSERVATION.pulse.v2`, `OBSERVATION.body_temperature.v2`, `OBSERVATION.head_circumference.v1`, `OBSERVATION.pulse_oximetry.v1`, `OBSERVATION.body_mass_index.v2`, `OBSERVATION.blood_pressure.v2` |
| `carePlans` | A.2.9 | Plan of Care | `ACTION.care_plan.v0`, `INSTRUCTION.service_request.v1` |

Each archetype id is `openEHR-EHR-` followed by the name in the table. The
Xt-EHR observation results are "measurements, laboratory results, anatomic
pathology results, radiology results or other imaging or clinical
results", so they take the template's vital signs as well as its diagnostic
results. Four crosswalk sections have no query:

- **The medical alert (A.2.1.2), a gap.** The template has no alert section,
  and no openEHR content is named to feed it.
- **The functional status (A.2.3.4), a gap.** The template's Functional
  Status section names only `EVALUATION.problem_diagnosis.v1` and
  `EVALUATION.clinical_synopsis.v1`, which do not tell a functional status
  from any other problem or impression, so a query on them would put every
  problem in the section.
- **The travel history (A.2.7.1).** The template has no travel section.
- **The patient story.** The section carries a note alone and no entry.

The selection is a superset for three reasons the review weighs. The
exclusion and absence statements are offered in several sections, so a
composition with "no known allergies" is selected for every section that
offers them, and the mapping places each statement by its content. The
template records both a medication statement and an immunisation statement
in `ACTION.medication.v1`, so the medication and immunisation queries select
the same compositions. The problem list and the past history share
`EVALUATION.problem_diagnosis.v1`.

## Shared hazards

| Id | Hazardous situation | Harm | Before | Control | Implemented by | After |
|---|---|---|---|---|---|---|
| X-01 | One component changes the other's output or behaviour (Art 30(1)(b)) | an access goes unrecorded, or a document changes, because of the other component | S3, P2 | two crates with no edge between them; only the composition root links both; the engine compiles neither | `app/ferrofed-engine/tests/it/architecture.rs` (`the_two_harmonised_components_are_independent_of_each_other`, `no_crate_but_the_composition_root_links_both_components`, `the_engine_compiles_neither_component_and_no_fhir`); the server tests that each output is unchanged by the other are planned with [#689](https://github.com/FerroHEALTH/FerroFED/issues/689) | S3, P1 at the crate level |

## The completeness default, re-examined

The interoperability component's document is all-or-nothing by default,
with a partial document on request and every silent member named, as §11
sets for a federated query. That default was to be re-examined here.

It holds. I-01 is the most severe hazard a federated summary adds, and the
default makes the summary fail visibly when a member that may hold the
patient's data is silent. A partial document is no less safe than a partial
query answer as long as every silent member is named in the document and no
section claims "no information" for a member that did not answer, which are
I-01's controls. The cost is L-02's kind: an answer withheld while one
member is down. The [claims review](claims-review.md#the-completeness-default)
assessed that cost against Annex II, point 2.5, and found the partial answer
one header away.

## Overall residual risk

- **Logging component:** L-01, L-02, L-03, L-05, L-06, L-08 and L-09 are
  controlled and tested; L-09 depends on the deployment naming its
  emergency purposes. L-04 is open at S2. L-07 is open at S3 and is
  carried by the deployment's Audit Record Repository until
  [#660](https://github.com/FerroHEALTH/FerroFED/issues/660) lands. The
  component records every access the gateway serves today.
- **Interoperability component:** I-07 and I-08 are controlled. Every other hazard is
  open, most at S4, so the component is not offered for its Annex II purpose
  and the gateway does not serve it. The overall residual risk of the
  component is evaluated again when the issues above close.

The [instructions for use](../operate/instructions-for-use.md#limitations)
name each open hazard a professional user needs to know (Art 28(b)).

## Review and change

- **At each release.** The release cut reviews this file when the release
  changes either component, its mapping pins or the query path it rides
  ([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
  Before the tag), and records the review below. A change to the section
  catalogue or the mapping pin is not merged without a review of I-01 to
  I-05.
- **From the field.** Every complaint and every possible serious incident
  is checked against this file, and a new hazard or a control that failed
  enters it ([Complaints and incidents](post-market.md)).
- **When an implementing act is adopted.** The exchange format (Art 15(1))
  and the common specifications (Art 36(1)) can change what a component must
  do; their adoption reopens the hazards they touch.

| Reviewed on | Release | Change | Hazards touched |
|---|---|---|---|
| 2026-10-06 | before 0.0.10 | file opened | all |
| 2026-10-06 | before 0.0.10 | the patient summary section catalogue: the crosswalk of `eehrxf` 0.0.4 ([#777](https://github.com/FerroHEALTH/FerroFED/issues/777)), with the medical alert and the functional status open | I-03; I-01, I-02, I-04 and I-05 unchanged, because no document is built yet |
| 2026-10-06 | before 0.0.10 | the section queries ([#776](https://github.com/FerroHEALTH/FerroFED/issues/776)): one read-only stored query per section, selecting by the archetypes the openEHR International Patient Summary template names (pending the clinical review), with the medical alert, the functional status, the travel history and the patient story unselected | I-07 controlled; I-03 (the selection and its gaps); I-01, I-02, I-04 and I-05 unchanged, because no document is built yet |
