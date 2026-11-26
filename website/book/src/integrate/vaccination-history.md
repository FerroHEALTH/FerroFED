<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The vaccination history over FHIR

MyHealth@EU Requirements Catalogue v10.0.0 §1.24 asks the contact point of
the country of affiliation to make a patient's vaccination history
available to the health professional. The gateway writes it from the
members' openEHR immunisation lists, in the MyHealth@EU Vaccination
10.0.0-preview form, and the
[document list](patient-summary.md#the-document-list) names it beside the
patient summary, under the same admission, treatment relationship and
access record as the other documents.

## Declaring the template

Declare the template of the members' compositions that hold the
immunisations section in the `[fhir.vaccination]` table:

```toml
[fhir.vaccination]
template = "/etc/ferrofed/templates/ferrobridge_patient_summary.adlt"
archetypes = [
  "/etc/ferrofed/archetypes/openEHR-EHR-COMPOSITION.health_summary.v1.adl",
  "/etc/ferrofed/archetypes/openEHR-EHR-SECTION.immunisation_list.v0.adl",
  "/etc/ferrofed/archetypes/openEHR-EHR-ACTION.medication.v1.adl",
  # and every archetype the template specialises or fills a slot with
]
```

- `template` is an OPT 1.4 `.opt`, or an ADL 2 template beside the
  `archetypes` it specialises or fills a slot with. The shipped EuVAC
  mapping of `eehrxf` maps FerroBRIDGE's patient summary template,
  `openEHR-EHR-COMPOSITION.ferrobridge_patient_summary.v1.0.0`, whose
  immunisations section is the CKM vaccination list
  (`openEHR-EHR-SECTION.immunisation_list.v0`).
- The configuration refuses a file it cannot read and a template the
  mapping does not compile against. Without the table, the list offers no
  vaccination history.
- The history is written in the record's language-region tag, held to
  `eHDSILanguage` by the terminology server of `[eehrxf.terminology]`, as
  the [discharge report](discharge-reports.md) is.

## One history per attested immunisation list

The list runs the reserved stored query
`eu.ferrofed.eehrxf::vaccination-history` on every member the patient
resolves at, by the member's own `ehr_id` alone. It selects every
composition that holds the immunisations section, with the attestations of
its version (`VERSION v CONTAINS COMPOSITION c`, `v/attestations`). Each
such composition is the member's persistent immunisation list, so the list
names one history per composition per member: a history merged across
members would have no one legal authenticator.

Each history is a `DocumentReference` typed LOINC `11369-6` (History of
Immunization note), the type `composition-euvac-myhealtheu` fixes, with no
priority category, since vaccination is none of the Art 14(1) categories.
Its format is `bundle-euvac-myhealtheu`, and its version, language and
period are the composition's. A `type` of `11369-6`, alone or in the LOINC
system, or `http://loinc.org|`, admits it.

The list names a history unavailable, in an `outcome` entry with the
reason, and never writes it, when:

- no person attested the record's version: the version carries no
  attestation, only a pending one, or only one whose committer is no named
  `PARTY_IDENTIFIED`;
- the identity binding holds no name or no birth date of the patient, which
  MyHealth@EU Core `patient-myhealtheu-core` requires.

## Reading the history

Reading the URL asks that member alone for the version's `ORIGINAL_VERSION`
(`GET /ehr/{ehr_id}/versioned_composition/{versioned_object_uid}/version/{version_uid}`),
which carries the composition and its attestations, and writes a
`bundle-euvac-myhealtheu`:

- its `composition-euvac-myhealtheu` is authored by the gateway's `Device`
  and the operator, its custodian the member's organisation, about the
  `Patient` the identity binding names behind the member's `ehr_id`;
- each person who attested the version is a `legalAuthenticator` attester,
  mode `legal`, at the attestation's `time_committed`, its party a
  `Practitioner` claiming `practitioner-myhealtheu-core`. The legal
  authenticator comes from the record's own attestation (openEHR RM Common
  `ATTESTATION`): the composer is the record's author, and a configured
  operator would claim an attestation that never happened;
- its immunisations section lists each dose the mapping writes as a
  `vaccination-myhealtheu-euvac`, and one `Provenance` names the member.

A version no person attested, or a patient with no name or birth date,
answers `424` with the reason. Each list and read is recorded in the access
log under `Patient-Summaries`, since Regulation (EU) 2025/327 recital 25
counts vaccination status as part of a patient summary.
