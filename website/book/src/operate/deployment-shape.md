<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED runs beside

There is nothing to run yet. This page describes what a running gateway will
need around it, taken from the roles the specification names, so an operator
can see the shape before the software exists.

## The services a gateway consumes

| Role | What it does | Proposed binding |
|---|---|---|
| Member CDRs | answer standard AQL scoped to one `ehr_id`, and the follow-up reads and writes routed to them | openEHR ITS-REST 1.1.0 |
| Identifier cross-reference | maps a patient identifier to each node's local `ehr_id`, or reports it not found | IHE PIXm |
| Localization (optional) | returns the candidate communities for a patient; without it the gateway asks every known node | IHE XCPD |
| Addressing | resolves each community to its cross-reference service and CDR base URLs | IHE mCSD |
| Authentication and authorization | authenticates the client, and the gateway to each node | the security profiles of §13 |

The specification references the internals of each service out (§2.2): how
an MPI matches identities, how a locator decides where data is, and the
transport trust framework all belong to their own profiles. A region may
supply its own realisation; Annex B describes the Dutch Generic Functions as
one.

## What the gateway keeps

The gateway holds no clinical data. It keeps the registry of organisations,
endpoints and the `system_id` mapping that routing depends on (§3.1, N21), and,
if the deployment offers it, the federated stored-query definitions it is
authoritative for (§12.7). Where that state lives and how it is stored is open
on the research program.

## Failure behaviour you should know before you run it

- When a node that was asked does not answer, the default is to fail the
  query. A client that wants flagged partial rows has to ask for best-effort
  explicitly (§11.4).
- Every response, a failing one included, reports each node in scope with a
  status such as `active`, `offline`, `time-out` or `not-resolved` (§11.1).
- Consent is enforced by each node before it releases data. A node's refusal
  is reported; the gateway never treats its own pre-filter as the only gate
  (N27).
