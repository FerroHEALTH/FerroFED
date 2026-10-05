<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Fuzz targets

`cargo fuzz` (libFuzzer) over the inputs a caller controls before anything
else reads them (#134). This crate sits outside the workspace, with its own
lockfile, because cargo-fuzz needs a nightly toolchain for its sanitizer
flags; nothing built here ships.

| Target | Entry point | Seeds |
|---|---|---|
| `aql_rewrite` | `openehr_federation::aql::analyse` over AQL text and parameters, then `for_node` | the façade query of every reference golden case, `OFFSET` pages under both strategies, and the `FROM ENDPOINT` and `ORGANISATION` directive |
| `adhoc_query` | the ITS-REST `AdhocQueryExecute` decode, the façade's `query_parameters` intake, then `analyse` | the same queries as request bodies, bare, with paging and a parameter, and with an `offset` member past zero |
| `result_set_meta` | the ITS-REST `RESULT_SET` decode, then `envelope::read` of `meta.federation` | the specification's section 9.4 example |
| `options_root` | the `OPTIONS {base}/` body decode | the specification's section 7a.2 example |

A finding is a panic, an abort or a hang. An `Err` or a refusal is the code
doing its job and is never a finding. Two targets also assert properties:

- `aql_rewrite` checks the identifier-hygiene property of §5.4.1 (N33) on
  every query the rewrite accepts. No string or integer literal of a node
  query carries the patient identifier. Given back its subject predicate, the
  node query must not be refused for an identifier elsewhere, so a string
  function that rebuilds the identifier (`CONCAT`, `CONCAT_WS`, `SUBSTRING`)
  is caught by the crate's own folding.
- `result_set_meta` and `options_root` check that a body which reads also
  survives its own encoding unchanged.

The input of `aql_rewrite` is the AQL text up to the first NUL byte. The bytes
after it choose the `OFFSET` strategy, the parameter values, the paging
members and the targeting. The input of `adhoc_query` is the request body up
to the first NUL byte, and the bytes after it choose the `OFFSET` strategy.
For both, an odd first byte after the NUL selects `reject`, and anything else
selects `bounded` with a window drawn from the following bytes (§11.6.2, N39).
An input with nothing after the body runs the server default, `bounded` with
a window of 1000 rows, so both paths through the paging are fuzzed.

## Seeds

`scripts/fuzz/seeds.sh` writes every `fuzz/seeds/<target>/gen-*` file from the
vendored corpora, and `scripts/fuzz/seeds.sh --check` fails when they are out
of date. Never hand-edit a `gen-*` file. A regression seed committed after a
finding carries any other name, so the generator leaves it alone, and so does a
hand-written seed for a path the corpora do not reach: the `entry-*` carrier
seeds, the `offset-*` pages, which end in `\0\x01` where they select
`reject`, and the `directive-*` queries, which carry the `ORGANISATION`
selector and a directive with no variable.

## Running a target

```sh
mkdir -p fuzz/corpus/aql_rewrite && cp fuzz/seeds/aql_rewrite/* fuzz/corpus/aql_rewrite/
cargo +nightly fuzz run aql_rewrite fuzz/corpus/aql_rewrite -- -max_total_time=60
```

The lane is `.github/workflows/fuzz.yml`: weekly and on dispatch, and on a pull
request that touches the code a target reads. A reproducing input is uploaded
as a run artifact and becomes a `bug` issue with the input attached.
