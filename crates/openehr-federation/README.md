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

The `aql` and `merge` modules hold their place and land with their FerroFED
issues. The rest of this page describes the wire types.

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
