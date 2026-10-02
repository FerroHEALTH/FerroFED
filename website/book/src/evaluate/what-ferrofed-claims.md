<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED claims

FerroFED claims what a release ships and a test holds. This page lists the
standing decisions, what the latest release does, what is on `main` for the
next one, and what is planned, so you can tell the four apart.

## Decided

The architecture of record was decided on 2026-10-01 and is
[`docs/architecture.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/architecture.md).
These decisions hold across every release:

- **The specification is the authority.** The Federation Tier with AQL text
  decides; its reference implementation is read as evidence, never as an
  oracle. Where the specification is silent, the decision is recorded as
  FerroFED's own and labelled that way in the code and the documentation.
- **The openEHR surface comes from the published `openehr-*` crates.** The
  ITS-REST contract, the AQL parser and printer, the RM and the typed
  identifiers are those crates. A gap in one of them is fixed in that crate.
- **No clinical data of its own.** The gateway holds the registry, the index
  and the stored-query definitions it is authoritative for. The record stays on
  the nodes.
- **Identifier hygiene is a hard rule.** Nothing the gateway composes for a
  node carries a directly identifying patient identifier, and every carrier the
  specification names gets a negative test.
- **Pure Rust, a single binary**, as across the FerroHEALTH family.

## In the latest release

v0.0.3 ships the first federated query and identity resolution:

- `POST {base}/v1/query/aql` answers one ITS-REST `RESULT_SET` over every
  member of a static registry, with no federation syntax needed (N1, CP-1).
- The patient is resolved outside AQL through an IHE PIXm PIX Manager (ITI-83),
  or a development cross-reference, on either patient-identifier carrier, and
  each member that knows the patient receives standard AQL keyed on its own
  `ehr_id` (§5, §7.1, N7, N33).
- The rewrite refuses a query that would carry the identifier to a node, and an
  outbound gate checks every request again before it leaves (§5.4, CP-26).
- `meta.federation` reports every endpoint with its status, and under the
  all-or-nothing default an `offline`, `time-out` or `node-error` node fails
  the query (§11.1 to §11.3, N16, N37).

## On `main`, for the next release

The [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
lists what has merged since v0.0.3 under `[Unreleased]`: among it the full
per-endpoint report, the best-effort completeness opt-in, the client's
`Prefer: wait` budget, `ORDER BY` with `LIMIT` merged across nodes,
`SELECT DISTINCT` across nodes, `OFFSET` paging (§9.5, §11.2 to §11.6),
and the §11.2 status mapping with the stable error codes of
[Errors and status codes](../integrate/errors.md), on the `openehr-*` crates
at the pin [Pinned versions](versions.md) records.

## Planned

Each milestone on the
[roadmap](https://github.com/FerroHEALTH/FerroFED/milestones) is a release,
and every issue in it names the sections it answers:

- the rest of the federated answer: aggregates and de-duplication (v0.0.4,
  §10, §11.6.3);
- the ITS-REST surface and follow-up routing to the owning CDR (v0.0.5, §7a,
  §12);
- targeting and the `OPTIONS {base}/` self-description (v0.0.6, §8, §7a.2);
- definitions, the stored-query registry and membership (v0.0.7, §12.6,
  §12.7, §12b);
- security and the bindings, the Dutch Generic Functions among them (v0.0.8,
  §13 to §15, Annex A, Annex B);
- every conformance point scored (v0.0.9, §16, §17).

## Not claimed

FerroFED claims a conformance point only when a test carries its marker and CI
runs it. The [conformance matrix](conformance.md) records where each point
stands: every point, its requirements and tracks as the specification states
them, and the issue that scores it. The specification is a release candidate;
when 1.0 is published, the vendored text is re-pinned and every citation on
the tracker is checked against it
([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)).
