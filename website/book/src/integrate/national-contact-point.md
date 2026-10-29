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

[[trust.issuer]]
issuer = "https://ncp.example.org"

[trust.issuer.national_contact_point]
family_name = "family_name"
given_name = "given_name"
country_code = "country_code"
professional_issuing_authority = "hp_issuing_authority_name"
provider_issuing_authority = "hcp_issuing_authority_name"
provider_address = "healthcare_provider_address"
correlation_header = "ncp-correlation-id"
```

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

## Failures the connector relays

The contact points exchange a common set of exceptions, and the national
answer travels with the one the contact point sends on (Requirements
Catalogue v10.0.0, §1.25). Every failure of the face is an
`OperationOutcome` whose status and issue type tell the connector which kind
it is:

| What happened | Status and issue type | Temporary? |
|---|---|---|
| A member, the identity binding or the access record could not answer in time or at all | `503` `transient`, `504` `timeout` | yes, the connector may ask again |
| A member answered with an error, named in the outcome | `424` `incomplete` | the member's, which the outcome names |
| The owner of a prescription failed on a dispensation or could not be reached | `502` `exception`, the member's own issues after the gateway's when it sent some | unknown: read the prescription before sending again |
| The request is malformed or misses a parameter | `400` `invalid` | no |
| The token misses an Annex attribute; the message names each missing one | `403` `forbidden` | no, until the contact point sends it |
| No member holds the patient, or the prescription | an empty `searchset`, or `404` `not-found` | no |
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
