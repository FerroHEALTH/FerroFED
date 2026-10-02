<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# openehr-federation

The openEHR Federation Tier with AQL specification in Rust: one crate for the
specification, with a feature per layer.

| Feature | What it adds |
|---|---|
| (always on) | the federation wire types, held to the two published JSON Schemas |
| `aql` | the §7 rewrite of a client query into one `ehr_id`-scoped query per node, with identifier hygiene (§5.4), on the `openehr-query` syntax tree |
| `merge` | the §9 to §11 merge of node answers: `ORDER BY` with `LIMIT`, `DISTINCT`, version-identity dedup, decomposable aggregates |

The `merge` feature carries `ORDER BY` with `LIMIT` today; `DISTINCT`, dedup and
the decomposable aggregates follow in build order.

## The `merge` feature

`merge::merge` takes the rows each node answered and the `order::ResultOrder`
the rewrite wrote into the node queries, and returns the federated rows with
the endpoints whose answer it could not use (§11.6.1, N39). The rows are put
in one total order: the `ORDER BY` keys (null greatest, numbers exactly,
complete date-times by instant, strings by code point, `DV_ORDERED` values by
`openehr-rm`), then the endpoint id, then the uid, and cut at the `LIMIT`, which
every node was sent unchanged. A node that returned `n` rows out of that order
is refused, and the gateway reports it `node-error`.

## The `aql` feature

`aql::analyse` takes the client's AQL, binds the ITS-REST `query_parameters`
into the `openehr-query` syntax tree, and finds the patient on the
`EHR_STATUS.subject.external_ref` carrier (§7). A patient query then prints one
node query per resolved `ehr_id`, with the patient predicate replaced by
`<ehr>/ehr_id/value = '<ehr_id>'` and a selected subject column left to
re-injection (N5). Every transformation is on the syntax tree, printed with
`printer::to_aql`.

The identifier never reaches a node query (§5.4.1, N33). A query that cannot
be reduced to one `ehr_id` scope per node, or in which the identifier appears
anywhere else, is refused with a typed `400` that locates the offending text by
byte range and never quotes it. The `ENTRY`-level subject carrier is refused
until it is accepted as resolution input.

## The wire types

A federation gateway answers an AQL query with an ordinary openEHR ITS-REST
`RESULT_SET` and adds one member to its open `meta`: `meta.federation`, the
per-endpoint record of what happened to each node. This crate models:

- `meta.federation` with `complete`, `endpoints[]`, `timeout` and `dedup`
  (§9.1, §9.5, §11.4), where `complete` is derived from the statuses rather
  than set by hand;
- the per-query endpoint status vocabulary of §11.1, including `node-error`,
  with the `error` and `latency_ms` obligations of N40 made part of each
  status's shape;
- the `OPTIONS {base}/` self-description (§7a.2, N30) and its two schema
  conditionals;
- the federation HTTP header names (§7a.3, §8.4, §10, §11.4);
- the one seam into the `openehr-its` `ResultSetMetadata`, which refuses the
  flat and `_`-prefixed forms §9.1 forbids (CP-35).

Every object in the schemas is open, so each type keeps the members it does
not model and writes them back. The tests validate every emitted body against
the vendored schemas, fail on drift when a re-pinned schema adds a member, and
pin the rules no schema can state.

The specification release this crate implements is in `FEDERATION_SPEC`, and
the AQL release the `aql` feature rewrites is in `AQL`.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
