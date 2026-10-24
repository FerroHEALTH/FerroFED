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

The MyHealth@EU NCPeH API guide 9.1.0 specifies only the exchange between
two contact points: "It is out of scope of this guide to define the
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
operator can check what the gateway expects.

## What the connector calls

The connector uses the routes of the FHIR face, each with its token and,
when you declared one, its correlation header:

| Route | What it answers |
|---|---|
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

The end-to-end test `e2e::national` drives these routes from the harness
national connector of `tools/ferrofed-testkit` over two FerroEHR nodes.

## What stays with the contact point

FerroFED answers the national connector in the exchange format of the FHIR
face, and does nothing on the contact point's side of the connector. These
stay with the contact point:

- **Patient identification.** The contact point identifies the patient
  against its Member State's identity services and asks FerroFED by the
  confirmed identifier. FerroFED answers no identification request and
  searches no demographics.
- **The eHDSI CDA.** FerroFED emits no eHDSI CDA pivot document and no
  national CDA. Regulation (EU) 2025/327 recital 26 allows different
  profiles at the level of EHR systems and of contact points, and the
  transformation to the contact point's profile is the contact point's.
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
existing CDA services move to FHIR over time.

The MyHealth@EU Requirements Catalogue, release 9.0.0 of 2025-03-24, is
available only with authorisation from DG SANTE behind EU Login (D6.1 §5),
and Commission Decision 2011/833/EU does not let a privileged-access document
be copied here without a written grant. FerroFED records it for manual
retrieval and has not read it. When the act is adopted, or the catalogue
can be read, the decisions behind this page are checked again.
