<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# A national contact point

A national contact point for digital health connects a Member State's
healthcare providers to MyHealth@EU and answers the requests that
health professionals of other Member States send through it (Regulation
(EU) 2025/327 Art 23, vendored as
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml)).
The part of a contact point that reaches into its own country is its
national connector. FerroFED is one of the national infrastructures a
national connector can call: it serves the connector on the FHIR face, the
same interface every other client of the European exchange format uses.

The MyHealth@EU NCPeH API guide (9.1.0, and its current build 9.1.1-ci)
specifies only the exchange between two contact points: "It is out of scope of this guide to define the
interfaces with the National Connector and/or between the National
Connector and National Infrastructure". Implementing Regulation (EU)
2026/2083 Art 3(6) leaves "the operation of national infrastructures" with
the contact point. How a national connector reaches FerroFED is therefore
FerroFED's own design, recorded in `docs/architecture.md` (decisions A67 to
A72).

## Connecting a national connector

You connect a contact point in three steps:

1. Serve the FHIR face under `[fhir]`, with the members that serve
   prescriptions and dispensations in `[[fhir.serve]]`
   ([The patient summary over FHIR](patient-summary.md),
   [Prescriptions and dispensations over FHIR](prescriptions.md)).
2. Trust the issuer of the connector's tokens under `[[auth.issuer]]` and
   `[[trust.issuer]]`, as for any client
   ([Authentication](../operate/authentication.md)).
3. Declare that issuer a national contact point with
   `[trust.issuer.national_contact_point]`, naming the claims its tokens
   carry the Implementing Regulation (EU) 2026/2099 Annex attributes in
   ([National contact points](../operate/authentication.md#national-contact-points)).
4. Give FerroFED its own PostgreSQL store with `[store]`, which a
   contact-point issuer needs; without it the gateway refuses to start
   ([FerroFED's own store](../operate/store.md)).

```toml
[fhir]
base = "/fhir"

[[fhir.serve]]
category = "Electronic-Prescriptions"
members = ["node-a", "node-b"]

[[fhir.serve]]
category = "Electronic-Dispensations"
members = ["node-a", "node-b"]

[[auth.issuer]]
issuer = "https://ncp.example.org"
kind = "persons"
audience = ["https://gateway.example.org/fed"]

[[trust.issuer]]
issuer = "https://ncp.example.org"

[trust.issuer.national_contact_point]
family_name = "family_name"
given_name = "given_name"
country_code = "country_code"
professional_issuing_authority = "hp_issuing_authority_name"
provider_issuing_authority = "hcp_issuing_authority_name"
provider_address = "healthcare_provider_address"
trc_patient = "trc_patient"
correlation_header = "ncp-correlation-id"

[store]
url_file = "/run/secrets/ferrofed/store-url"
```

`trc_patient` names the claim that carries the patient of the treatment
relationship confirmation (TRC): its `subject-id` (eHDSI SAML Profile
10.0.0 §4.3), as one `system|value` token, which your authorization server
copies from the TRC it validated. Every claim name is required.

`ferrofed config check` reads the table, and `OPTIONS {base}/` declares the
contact point under `federation.national_contact_point`, so the connector's
operator can check what the gateway expects. You see the running values on
the operator console's Configuration page, on its Authentication tab, and in
the running configuration of the admin listener
([The running configuration](../operate/configuration-revisions.md#the-running-configuration)):
each claim name with its key, the correlation header and the added
countries, and any secret by the file it is read from alone.

## What the connector calls

The connector uses the routes of the FHIR face, each with its token and,
when you declared one, its correlation header:

| Route | What it answers |
|---|---|
| `GET {fhir-base}/Patient?identifier=system\|value` | the patient lookup: the `Patient` the identity binding holds, or the empty `searchset` |
| `GET {fhir-base}/Patient/$summary?identifier=system\|value` | the patient summary built from every member that holds the patient |
| `GET {fhir-base}/DocumentReference?patient.identifier=system\|value` | the document list |
| `GET {fhir-base}/MedicationRequest?patient.identifier=system\|value` | the patient's prescriptions at the serving members |
| `GET {fhir-base}/MedicationRequest/{id}` | one prescription |
| `POST {fhir-base}/MedicationDispense` | a dispensation, written to the member that owns its prescription |

The connector names the patient by the national identifier its own
identity services confirmed, with that identifier's system. The gateway
resolves it as it resolves any subject, through the configured identity
binding, and answers a patient no member holds and a patient every member
restricts alike.

### The treatment relationship binding

The patient lookup is the identification step and needs no TRC. Every other
route reaches patient data, and the gateway holds it to the patient the
token's `trc_patient` claim names (eHDSI XCA Profile 8.0.1 §4.2.1 and
§4.2.2):

- A token with no `trc_patient` claim, or one that is not `system|value`, is
  refused `403 forbidden` with the diagnostics `trc-missing`, and nothing is
  asked of anyone.
- The patient a summary, a document list or a prescription search names must
  be the TRC's patient, compared on the system and the value before the
  identity binding or any member is asked. A dispensation's `subject` must
  carry the TRC's identifier.
- The gateway resolves the TRC's patient at every member before the request
  goes on. A read of a prescription, a dispensation or a document by its id,
  and a withdrawal, reaches its member only when the resource sits in that
  patient's own EHR there.
- A request outside the TRC's patient, a patient no member holds, and any
  other `not-found` on these routes all get one answer: `404 not-found`,
  whose `issue.details` carries the refusal `trc-patient-mismatch`
  (`https://ferrofed.eu/fhir/CodeSystem/refusal`) and the service's
  Requirements Catalogue 10.0.0 §1.25 code
  (`https://ferrofed.eu/fhir/CodeSystem/myhealth-eu-exception`):
  `ERROR_PS_NOT_FOUND` for the summary and its documents,
  `ERROR_ORCD_NOT_FOUND` for a laboratory report, and `ERROR_EP_NOT_FOUND`
  for prescriptions and dispensations. Nothing in it tells the cases apart,
  so the connector answers "No Data" as XCA §4.2.2 asks. The legacy numeric
  codes of the NCPeH Components Specifications 10.0.0 appendix 6.4 are never
  used.
- A request outside the TRC's patient is a security event. It is logged
  under the target `ferrofed::security` as `trc-patient-mismatch` and counted
  in the security metrics. The event names the contact point, the
  professional (`hp_identifier`, its issuing authority and country) and the
  request id, and names both patients by pseudonym alone: the first 16 bytes
  of an HMAC-SHA256 under a key the process generates at start and never
  writes anywhere (eHDSI Section I Security Policies 10.0.0 SR 5). The
  contact point's own audit trail carries the patient, joined by the
  correlation identifier. Blocking the professional, which XCA §4.2.2 asks
  as a SHOULD, stays with the contact point.
- A TRC patient the cross-reference cannot resolve at every member, or a
  deployment with no cross-reference, is answered `424`
  `patient-context-unavailable` with nothing sent, and logged under
  `ferrofed::security` as `trc-patient-unresolved` with the cause.

No specification governs the binding across members, the code systems the
codes travel under, or the pseudonym: they are FerroFED's own design over the
XCA rule that the contact point of the country of affiliation compares the
patients. The FHIR face is the contact point's path.

### The binding on the ITS-REST face

A contact point's token is admitted to the ITS-REST face as well, and the
gateway holds a patient-data request there to the same TRC patient, in the
answers ITS-REST gives (openEHR ITS-REST 1.1.0; eHDSI XCA Profile 8.0.1
§4.2.1 and §4.2.2):

- A token with no `trc_patient` claim is refused `403` with the code
  `trc-missing` on every route that reaches patient data: the query routes,
  the EHR API and the DEMOGRAPHIC API. A definition request reaches no
  patient data and needs none.
- A query, ad hoc or stored, sent by `POST` or `GET`, must name the TRC's
  patient, compared on the namespace and the value before the demographics
  supplier, a localizer, the consent pre-filter or any member is asked. A
  query scoped to one `ehr_id` (N29) must be scoped to one of that patient's
  own `ehr_id`s at the member it goes to.
- A request under `{base}/v1/ehr/{ehr_id}`, reads and writes alike, goes
  only to the member where that `ehr_id` is the TRC patient's own EHR,
  located through the TRC patient's pairs: the `ehr_id` index is not read
  and no member is probed. `GET {base}/v1/ehr` by subject must name the TRC's
  patient.
- Another patient, an EHR of another patient, a TRC patient no member holds,
  and a TRC patient every member that holds it restricts all get one answer:
  `404` with the code `subject-unavailable`, the answer an unavailable
  subject gets when consent exclusions are withheld, which a contact point's
  request always is. Nothing in it tells the cases apart, and no member is
  sent anything.
- A query that names no patient and is scoped to no `ehr_id`, the creation
  of an EHR, and a DEMOGRAPHIC request name nothing a TRC binds: each is
  refused `403` with the code `trc-unbound`, with nothing sent.
- Every request outside the TRC's patient is the same `trc-patient-mismatch`
  security event the FHIR face records, naming the professional and both
  patients by pseudonym alone.

ITS-REST has no MyHealth@EU service, so its answers carry no §1.25
`*_NOT_FOUND` code ([Error codes](errors.md#subject-unavailable)).

For every request of the contact point, the gateway:

- requires every Annex attribute and a purpose of use, and refuses a
  request that lacks one before any member is asked;
- withholds every consent exclusion, whatever `[federation.consent]
  disclose` says (Regulation (EU) 2025/327 Art 8, Art 11(5));
- writes one access record naming the foreign professional and provider,
  marked as asserted by the contact point, with the correlation identifier
  ([Audit](../operate/access-log.md#a-request-a-national-contact-point-relays));
- tells each member the foreign professional and provider in the caller's
  conveyance, and sends no patient identifier to any member (Federation
  Tier §5.4.1, N33).

The gateway never refuses a contact-point request because of the
professional's role or the point of care: it conveys both for the record and
for the member's information. The country of affiliation must not deny
access on them (MyHealth@EU Requirements Catalogue v10.0.0, 01 and 01.02),
so a member that applies its own release rule to a contact-point request
must not use them either.

## The emergency access limit

A health professional may ask for a patient's data in an emergency, when the
patient cannot confirm their will, with the purpose of use `EMERGENCY`
(eHDSI SAML Profile 10.0.0 §2.3). eHDSI Section II Security Services 10.0.0
§2.5 has the contact point of the patient's country provide such access
and limit it: "No more than five emergency access attempts per HP per
24-hour period", which each country may tighten. The NCPeH Architecture
Specifications 10.0.0 §5.1.9 make an emergency access past the
country-defined threshold a deny response. Implementing Regulation (EU)
2026/2083 Art 4(1) binds the contact points to these specifications.
FerroFED applies the limit on the national side, behind the contact point,
so the limit holds even though the contact point's own policy runs
elsewhere. Where and how the count is kept is FerroFED's own design.

A contact point's request is an emergency access when its token declares a
purpose of use that `[[access_log.emergency_purpose]]` names, the same rule
that marks the access record ([Emergency
access](../operate/access-log.md#emergency-access)). Declare the code your
contact point's tokens carry:

```toml
[[access_log.emergency_purpose]]
system = "urn:uuid:3bc18518-d305-46c2-a8d6-94bd59856e9e"   # the system your tokens name
code = "EMERGENCY"

[emergency]
threshold = 5        # 1 to 5: emergency accesses one professional is granted in one window
window_s = 86400     # 1 to 86400: the window, in seconds
```

| Key | Default | What it sets |
|---|---|---|
| `threshold` | `5` | how many emergency accesses one professional is granted in any window, 1 to 5 |
| `window_s` | `86400` | the window, in seconds, 1 to 86400 (24 hours) |

The defaults are the §2.5 baseline. A country may only tighten it, so
`config check`, the start and a reload refuse a `threshold` above 5 or a
`window_s` above 86400, naming §2.5:

```text
emergency.threshold is 6, above the baseline of 5 emergency access attempts per health professional per 24-hour period of eHDSI Section II Security Services 10.0.0 §2.5, which a country may only tighten
```

A `threshold` or a `window_s` of zero is refused too: §2.5 has every country
provide emergency access, so FerroFED never refuses it by default. A reload
applies a changed limit. `OPTIONS {base}/` declares the limit in effect
under `federation.national_contact_point.emergency`, as `threshold` and
`window_s`, so the contact point's operator can check what the gateway
holds its professionals to.

How the gateway counts:

- **Per professional.** The professional is the one the token relays: the
  contact point, `country_code`, the issuing authority and `hp_identifier`
  (Implementing Regulation (EU) 2026/2099 Annex Table 1).
- **Every request that reaches patient data.** The summary, the document
  list, a document, a prescription search and read, and a dispensation each
  count once. The patient lookup is the identification step and is never
  counted.
- **In any window.** The window slides: any span of `window_s` seconds holds
  at most `threshold` granted accesses of one professional. A denied
  attempt is not counted, so the professional is granted again once the
  oldest access leaves the window.
- **Across replicas and restarts.** The count lives in FerroFED's own store
  (`ferrofed.emergency_access`, [FerroFED's own store](../operate/store.md)),
  read and incremented in one statement, so replicas share one count and a
  restart keeps it. The table names each professional by a SHA-256 digest
  alone.

Past the limit the request is denied before any member is asked. On the
FHIR face the answer is `403 forbidden`, whose `issue.details` carries the
refusal `emergency-limit-reached`
(`https://ferrofed.eu/fhir/CodeSystem/refusal`) and `ERROR_SEC_GENERIC`
(`https://ferrofed.eu/fhir/CodeSystem/myhealth-eu-exception`; eHDSI XCA
Profile 8.0.1 §2.2.3, "Security validations for the request or the
requestor failed"). Its diagnostics note that too many attempts for an
emergency access have been made, as §2.5 asks. The access record of the
denied attempt carries the emergency mark, and its outcome description
names the anomaly warning `EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS`
(NCPeH Architecture §5.1.9), so the contact point and the national audit
see it. The denial is also the security event `emergency-limit-reached`,
logged under `ferrofed::security` and counted in the security metrics,
naming the contact point and the professional.

When the store does not answer, an emergency access cannot be counted, so
it is refused `503 emergency-uncounted` and no member is asked. A request
that is no emergency access never touches the count.

## Failures the connector relays

The contact points exchange a common set of exceptions, and the national
answer travels with the one the contact point sends on (Requirements
Catalogue v10.0.0, §1.25). Every failure of the face is an
`OperationOutcome` whose status and issue type tell the connector which kind
it is:

| What happened | Status and issue type | Temporary? |
|---|---|---|
| A member, the identity binding, the access record or the store that counts emergency accesses could not answer in time or at all | `503` `transient`, `504` `timeout` | yes, the connector may ask again |
| A member answered with an error, named in the outcome | `424` `incomplete` | the member's, which the outcome names |
| The owner of a prescription failed on a dispensation or could not be reached | `502` `exception`, the member's own issues after the gateway's when it sent some | unknown: read the prescription before sending again |
| The request is malformed or misses a parameter | `400` `invalid` | no |
| The token misses an Annex attribute; the message names each missing one | `403` `forbidden` | no, until the contact point sends it |
| The token names no TRC patient | `403` `forbidden`, `trc-missing` | no, until the contact point sends it |
| The professional reached the emergency access limit | `403` `forbidden`, `emergency-limit-reached` and `ERROR_SEC_GENERIC` | yes, once the oldest emergency access leaves the window |
| No member holds the patient, or the prescription, or the request is outside the TRC's patient | `404` `not-found`, `trc-patient-mismatch` and the service's `*_NOT_FOUND` code | no |
| The member that owns the prescription refused the dispensation | `409` `conflict`, the member's own issues after the gateway's | no |
| The gateway failed on its own | `500` `exception` | no |

A patient no member holds and a patient every member restricts answer
alike, so the connector reports both as a patient with no data.

The end-to-end test `e2e::national` drives these routes from the harness
national connector of `tools/ferrofed-testkit` over two FerroEHR nodes.

## What stays with the contact point

FerroFED answers the national connector in the exchange format of the FHIR
face, and does nothing on the contact point's side of the connector. These
stay with the contact point:

- **Patient identification.** The contact point identifies the patient
  against its Member State's identity services and asks FerroFED by the
  confirmed identifier. FerroFED searches no demographics. The connector
  may look the confirmed identifier up first with
  `GET {fhir-base}/Patient?identifier=`, the patient lookup Xt-EHR D5.1
  §3.4.6 asks of every producer EHR system and has a consumer use
  ([Looking a patient up](patient-summary.md#looking-a-patient-up)).
- **The eHDSI CDA and the PDF of the original.** FerroFED emits no eHDSI
  CDA pivot document, no national CDA and no PDF. The contact point of the
  country of affiliation passes on a PDF of the original summary in its
  language (Requirements Catalogue v10.0.0, 05.02); every section of the
  gateway's document carries that original information as its generated
  narrative, from which the contact point renders it. Regulation (EU) 2025/327 recital 26 allows different
  profiles at the level of EHR systems and of contact points, and the
  transformation to the contact point's profile is the contact point's.
  MyHealth@EU runs its Patient Summary and ePrescription services on CDA,
  and its FHIR guides for them are proofs of concept; the reference
  contact point, OpenNCP, validates and transcodes the CDA in its own
  Transformation Manager.
- **Transcoding and translation.** Implementing Regulation (EU) 2026/2083
  Art 3(4) and (5) give the mapping and translation of codes to the Member
  State, with the Commission's central terminology service.
- **HCP and treatment-relationship assertions, SMP and cross-border
  transport.** They belong to the exchange between contact points, which
  the NCPeH API guide specifies, and to the secure network the contact
  points use (2026/2083 recital 6). FerroFED builds no XCPD, XCA or XDR
  responder and reads no SAML assertion.
- **The exchange log.** 2026/2083 Art 15(1)(f)(ii) puts the log of the
  cross-border exchange on the contact point. FerroFED keeps its own access
  record, which the correlation identifier joins to that log.
- **The joint-controller role.** Regulation (EU) 2025/327 Art 23 makes the
  contact points joint controllers for the processing in MyHealth@EU
  (2026/2083 recital 19).

## The sources these decisions were checked against

The Commission has not adopted the implementing act on the exchange
format (Regulation (EU) 2025/327 Art 15(1)), and its Have your say page
(initiative 17512) lists the draft act as upcoming, with no text. The
closest published text is the Xt-EHR joint action's deliverables D6.1
(Patient Summary) and D6.2 (electronic prescription and dispensation),
which FerroFED reads as a draft (`scripts/vendor/xt-ehr.sh`). Their
cross-border flow agrees with the list above: the contact point of the
patient's country retrieves the summary from the national systems, the
patient's identity is confirmed before the request, and transformation and
translation happen at the contact point (D6.1 §8.2.1). D6.2 §6.2 says the
existing CDA services move to FHIR over time. D5.1 has the contact point
query the producer EHR systems through the interoperability component as
a consumer (§7.1.1), and asks each producer for a patient lookup, a
document list and a summary retrieval (§3.4.6, §3.5.4, §6.6). FerroFED
caches every published Xt-EHR deliverable.

The decisions were also checked against the public MyHealth@EU material,
vendored by `scripts/vendor/myhealth-eu.sh` and `scripts/vendor/eu.sh`:
the MyHealth@EU FHIR guides (core, Patient Summary, ePrescription and
eDispensation, laboratory, discharge and imaging reports, vaccination and
the NCPeH API), the eHDSI CDA reference test documents, and the OpenNCP
Transformation Manager's Schematrons and coded element list.

The decisions were checked again on 2026-10-29 against the MyHealth@EU
Requirements Catalogue v10.0.0 (W10-Operation Ready, 2026-03-06), which the
owner read under an EU Login reading grant: its requirements on health
professional identification (01), patient identification (02), the contact
point's operation and data protection (03, 04), the patient summary (05),
information quality (10), security, traceability and auditability (11),
non-repudiation (12), the summary use case and the exception handling. They
hold, with two amendments of FerroFED's own: the gateway never refuses on
the professional's role or point of care, and the PDF of the original is
the contact point's, rendered from the narrative. The catalogue is
privileged-access and FerroFED has no grant to reuse it (Commission
Decision 2011/833/EU Art 2(2)(c)), so this page and `docs/architecture.md`
cite it by version, section and requirement id only and carry none of its
text. This is FerroFED's own reading, not a MyHealth@EU compliance check.
The result for every requirement id, with its status and evidence, is on
[MyHealth@EU self-assessment](../evaluate/myhealth-eu.md). When the Art 15(1)
act is adopted, the decisions behind this page are checked again.
