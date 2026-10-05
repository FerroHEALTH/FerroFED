<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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
- [Conformance matrix](evaluate/conformance.md)
- [Obligations checklist](evaluate/obligations.md)
- [Pinned versions](evaluate/versions.md)
- [Licensing](evaluate/licensing.md)
- [Regulatory status](evaluate/regulatory-status.md)

# Operate

- [What FerroFED runs beside](operate/deployment-shape.md)
- [The container image and the quickstart](operate/container.md)
- [Configuration](operate/configuration.md)
  - [The registry](operate/registry.md)
  - [Identity resolution](operate/identity.md)
    - [Withholding consent exclusions](operate/consent-exclusions.md)
  - [The audit trail](operate/audit.md)
  - [Client authentication](operate/authentication.md)
    - [The §13.4 deployment decisions](operate/deployment-decisions.md)
  - [Onward credentials](operate/onward-credentials.md)
  - [Queries and API areas](operate/queries-and-areas.md)
- [Admitting a node](operate/admission.md)
- [Scoring a deployment](operate/conformance-run.md)
- [Health probes](operate/health.md)
- [Metrics](operate/metrics.md)
- [Tracing](operate/tracing.md)
- [The operator console (planned screens)](operate/operator-console.md)

# Integrate

- [The client contract](integrate/client-contract.md)
  - [Follow-ups](integrate/follow-ups.md)
  - [Templates, definitions and demographics](integrate/templates-and-demographics.md)
  - [Stored queries](integrate/stored-queries.md)
- [Errors and status codes](integrate/errors.md)

# Contribute

- [How the work is organised](contribute/how-the-work-is-organised.md)
- [Checks and gates](contribute/checks-and-gates.md)
- [Adding a country](contribute/adding-a-country.md)
