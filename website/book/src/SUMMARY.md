<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Summary

[Introduction](introduction.md)

# How it works

- [Overview](how-it-works/overview.md)
  - [A federated query, end to end](how-it-works/federated-query.md)
  - [Where the patient identifier stops](how-it-works/identifier-hygiene.md)
  - [Follow-ups and writes](how-it-works/follow-ups-and-writes.md)
  - [Definitions and stored queries](how-it-works/definitions.md)
  - [Trust and keys](how-it-works/trust-and-keys.md)
  - [Deployment](how-it-works/deployment.md)

# Evaluate

- [The Federation Tier with AQL](evaluate/the-federation-tier.md)
- [What FerroFED claims](evaluate/what-ferrofed-claims.md)
  - [Claims review](evaluate/claims-review.md)
- [Conformance statement](evaluate/conformance-statement.md)
  - [Conformance tests](evaluate/conformance-tests.md)
- [Conformance matrix](evaluate/conformance.md)
- [Obligations checklist](evaluate/obligations.md)
- [Pinned versions](evaluate/versions.md)
- [Licensing](evaluate/licensing.md)
- [Regulatory status](evaluate/regulatory-status.md)
  - [Information sheet](evaluate/information-sheet.md)
  - [CRA information to the user](evaluate/cra-user-information.md)
  - [Technical documentation](evaluate/technical-documentation.md)
    - [Annex II checklist](evaluate/annex-ii.md)
    - [Rights of natural persons](evaluate/natural-person-rights.md)
    - [Cybersecurity risk assessment](evaluate/cra-risk-assessment.md)
  - [Receiving a document](evaluate/receiving-documents.md)
  - [Clinical safety risk file](evaluate/clinical-safety.md)
  - [Complaints and incidents](evaluate/post-market.md)
- [Data protection](evaluate/data-protection.md)
- [Threat model](evaluate/threat-model.md)

# Operate

- [Instructions for use](operate/instructions-for-use.md)
- [A production deployment](operate/production.md)
- [What FerroFED runs beside](operate/deployment-shape.md)
- [The container image and the quickstart](operate/container.md)
- [Upgrading](operate/upgrading.md)
  - [Rollback](operate/rollback.md)
- [Decommissioning](operate/decommissioning.md)
- [Configuration](operate/configuration.md)
  - [The public address](operate/public-address.md)
  - [What the gateway sends a node](operate/node-requests.md)
  - [The registry](operate/registry.md)
    - [The registry in FHIR form](operate/registry-fhir.md)
  - [Identity resolution](operate/identity.md)
  - [Localization](operate/localization.md)
  - [Consent](operate/consent.md)
    - [Withholding consent exclusions](operate/consent-exclusions.md)
  - [The audit trail](operate/audit.md)
    - [The access log](operate/access-log.md)
    - [Labelling the access records](operate/access-labels.md)
  - [Client authentication](operate/authentication.md)
    - [The §13.4 deployment decisions](operate/deployment-decisions.md)
    - [What a node is told about the caller](operate/caller-token.md)
  - [Onward credentials](operate/onward-credentials.md)
  - [Queries and API areas](operate/queries-and-areas.md)
  - [Configuration revisions](operate/configuration-revisions.md)
- [Admitting a node](operate/admission.md)
- [Exporting the identity state](operate/identity-state-export.md)
- [Scoring a deployment](operate/conformance-run.md)
- [Health probes](operate/health.md)
- [Overload protection](operate/overload.md)
- [Metrics](operate/metrics.md)
- [Tracing](operate/tracing.md)
- [The operator console](operate/operator-console.md)
- [Hardening](operate/hardening.md)
- [Troubleshooting](operate/troubleshooting.md)

# Integrate

- [The client contract](integrate/client-contract.md)
  - [Follow-ups](integrate/follow-ups.md)
  - [Templates, definitions and demographics](integrate/templates-and-demographics.md)
  - [Stored queries](integrate/stored-queries.md)
- [The patient summary over FHIR](integrate/patient-summary.md)
- [Errors and status codes](integrate/errors.md)

# Contribute

- [How the work is organised](contribute/how-the-work-is-organised.md)
- [Checks and gates](contribute/checks-and-gates.md)
- [Adding a country](contribute/adding-a-country.md)
