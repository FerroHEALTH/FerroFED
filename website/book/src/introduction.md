<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroFED is a pure-Rust openEHR federation gateway, one of the
[FerroHEALTH](https://ferrohealth.eu/) family. It sits in front of several
openEHR CDRs as a transparent ITS-REST intermediary. A client sends it an
ordinary AQL query with no federation syntax, and gets back an ordinary
ITS-REST result set whose `meta.federation` names each member with its
status. The gateway resolves the patient first, outside the query, through
an identifier cross-reference service, sends each member standard AQL scoped
to that member's own `ehr_id`, and merges the answers with each member's
provenance. No request it composes for a member carries a directly
identifying patient identifier, and it holds no clinical data of its own.

Beside that ITS-REST face, a FHIR face serves the same members in the
European electronic health record exchange format: the patient summary, the
laboratory, discharge and imaging reports, prescriptions and dispensations,
for any client and for a national contact point of MyHealth@EU.

FerroFED implements the openEHR Federation Working Group's
[Federation Tier with AQL](https://syntaric.github.io/openehr-federation-spec/)
specification, release 1.0.1, and reaches every member over openEHR
ITS-REST 1.1.0.

## Where to start

| You are | Start with | Then |
|---|---|---|
| An integrator writing a client | [Your first federated query](tutorials/first-federated-query.md) | [The client contract](integrate/client-contract.md), [Follow-ups](integrate/follow-ups.md), [Errors and status codes](integrate/errors.md), [The patient summary over FHIR](integrate/patient-summary.md) |
| A national contact point's integrator | [A national contact point](integrate/national-contact-point.md) | [Prescriptions and dispensations over FHIR](integrate/prescriptions.md), [National contact point issuers](operate/contact-point-issuers.md) |
| An operator who deploys and runs it | [A production deployment](operate/production.md) | [Configuration](operate/configuration.md), [Upgrading](operate/upgrading.md), [FerroFED's own store](operate/store.md), [Troubleshooting](operate/troubleshooting.md), the [command-line reference](reference/cli.md) |
| A decision maker, or compliance and procurement | [How FerroFED federates](explanation/how-ferrofed-federates.md) | [What FerroFED claims](evaluate/what-ferrofed-claims.md), [Regulatory status](evaluate/regulatory-status.md), [Conformance statement](evaluate/conformance-statement.md), [Licensing](evaluate/licensing.md) |
| A contributor | [How the work is organised](contribute/how-the-work-is-organised.md) | [Checks and gates](contribute/checks-and-gates.md) |

## How this book is organised

The book follows the [Diátaxis](https://diataxis.fr/) framework:

- **Tutorials** take you through a first task, step by step.
- **How-to guides** do one task each: integrating a client, and deploying
  and operating the gateway.
- **Reference** lists what the code holds: the commands, the error codes,
  the metrics, the conformance matrix and the version pins. The command-line
  reference is generated from the binary.
- **Explanation** says how the gateway works and why.
- **Compliance** is the record a release is assessed against: the claims,
  the regulatory status, the technical documentation and the risk files.

This book covers FerroFED alone. What every FerroHEALTH product shares is in
the [family book](https://ferrohealth.eu/docs/):
[how the products fit together](https://ferrohealth.eu/docs/architecture.html),
[the manufacturer](https://ferrohealth.eu/docs/manufacturer.html), Cadasto
B.V., with its [security policy](https://ferrohealth.eu/docs/security.html)
and the [Cyber Resilience Act](https://ferrohealth.eu/docs/cra.html) from the
manufacturer's side, and
[licensing and trademarks](https://ferrohealth.eu/docs/licensing.html).

The book describes the gateway as it is built on `main`. The
[latest release](https://github.com/FerroHEALTH/FerroFED/releases/latest)
ships the `ferrofed` binary for Linux, the signed image
`ghcr.io/ferrohealth/ferrofed`, and a `compose.yaml` that runs the gateway
in front of your own CDRs; the
[changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
says which release first shipped each part.
