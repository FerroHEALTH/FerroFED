<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The client contract

A client of a federation gateway is an ordinary openEHR client. This page sets
out what the specification promises that client. FerroFED serves the
federated query at `POST {base}/v1/query/aql` once a registry is configured;
every other ITS-REST path under `/v1/` answers `501` (N32).

## What a client sends

A conformant openEHR AQL request to `POST {base}/v1/query/aql`, where `{base}`
is the deployment's ITS-REST base URL; no prefix is mandated (§4.1, N28). The
patient is identified the openEHR way:

```sql
SELECT c/uid/value AS composition_id, c/context/start_time/value AS start_time
FROM EHR e CONTAINS COMPOSITION c
WHERE e/ehr_status/subject/external_ref/id/value = '12345'
  AND c/archetype_node_id = 'openEHR-EHR-COMPOSITION.encounter.v1'
```

No federation-specific syntax is needed for a basic patient query (§3.2, N1).
An optional AQL extension, `FROM ENDPOINT …` and `ORGANISATION …`, pins a
query to named systems for a client that wants it (§8).

## What a client gets back

An openEHR `RESULT_SET` exactly as ITS-REST 1.1.0 defines it, rows as ordered
arrays matched to `columns` (§9.1). A client that parses the AQL response of a
single CDR parses a federated one. Everything the federation adds lives in one
member of the open `meta` object, `meta.federation`:

- `complete`, whether the answer covers every node in scope (§11.4);
- `endpoints[]`, one entry per node with its status and provenance (§9.5,
  §11.1);
- `timeout`, the budget that applied (§11.5);
- `dedup`, the de-duplication policy that applied (§10).

By default you get every row every node returned, duplicates included
(§10.1, N15). Send `openEHR-federation-dedup: version-identity` to get one
copy of a composition version held at several nodes: the copy from the CDR
that created it when that CDR answered, else the one from the lowest
endpoint id. Version ids and system ids that differ only in case name the
same thing (openEHR BASE), and the kept row comes back as its node sent it.
`meta.federation.dedup` then names the endpoints whose
copies were dropped and counts the rows (§10.2, §10.3). Two versions of one
composition are two rows either way, and `none` states the default
explicitly. Any other value is refused `400` with the code `dedup-invalid`.

Read whether an answer is complete from `meta.federation.complete`, never
from the status code; the gateway emits no FHIR `OperationOutcome`, because
its answer is an ITS-REST `RESULT_SET` (§11.4, N17).

A request that fails answers the status §11.2 names and a stable code; the
[errors and status codes](errors.md) page lists every one.

## Follow-ups

A composition id in a result row is an `OBJECT_VERSION_ID`, which already
carries the `creating_system_id` of the CDR that created it. A follow-up read
or write sent to the gateway is routed to that CDR (§7.2, §12). A new object
is always created on one node the client names; creation across nodes is
refused (§2.3, N23).

## Self-description

`OPTIONS {base}/` returns the gateway's self-description, including what it
refuses rather than approximates, validated against the specification's
`options-root.schema.json` (§7a.2).
