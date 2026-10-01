<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier reference implementation

The whole repository tree, vendored verbatim by
`scripts/vendor/federation-ref.sh` (.claude/rules/vendored-inputs.md), less
the dependency manifest listed below. Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/syntaric/openehr-federation-ref>
- Pin: commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5`
- Declared version: `0.9.0-SNAPSHOT` (the `pom.xml` project version, which
  tracks the specification version it implements)
- Fetched: 2026-10-01
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file with its `NOTICE`
- Layout: the upstream paths, unchanged
- Files: 197, of which 139 are Java sources and 17 are AQL golden
  cases under `src/test/resources/aql-golden/`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `12bbe44dea3e3fb85b9a997f474c146e8c2235d23491f16aee948352c643853e`
- Read by: #18 (the golden cases adjudicated against the specification), #35 (the golden cases as a corpus test), #39 (the demo template and compositions as the e2e seed) and #94 (the differential run)

## Evidence, never an oracle

The specification in `docs/specs/federation-spec/` decides what a gateway does. This tree is
one working gateway, a Java and Spring Boot service, read for its golden AQL
rewrite cases, its schema copies, its synthetic demo data and its account of
where the specification needed a decision. It is never compiled here and no
code is copied from it: FerroFED is its own design under its own licence. A
disagreement between this tree and the specification is recorded as an
upstream-report issue, and the specification wins.

## Schema copies

The implementation tests against its own copies of the two published schemas.
Each is compared with the vendored specification at its pin:

| File | sha256 | Against the specification |
|---|---|---|
| `src/test/resources/spec-schemas/federated-result-set.schema.json` | `98804682d30b09d96d7b5ebe9e67a9e50844fc9fd709cad1425bce5fe06b0baf` | DIFFERS from the vendored specification (`abbcdd7f1481a6febcf5a211a987873420701991c93c89d780f660ceef1a643a`) |
| `src/test/resources/spec-schemas/options-root.schema.json` | `29a0dcd9fc16f50ddb9197bcc877fb071e6d85fd2b7160717f2065eb77586fd3` | identical to the vendored specification |

A difference is a finding about the implementation, recorded on the tracker
and never resolved by editing either copy. The specification's copy is the
one FerroFED validates against.

## Test keys

The RSA key pair below is a test fixture the upstream publishes for its own
token tests. It protects nothing, and no FerroFED test or deployment may use
it. It is recorded here so a secret scanner's finding on this path can be
matched against these hashes and dismissed.

| File | sha256 |
|---|---|
| `src/test/resources/keys/test-private.pem` | `f8ba836df3b5eca0a9adc3a9cc4967a8e88a52c802f8578aa5a7242a9cc91a1b` |
| `src/test/resources/keys/test-public.pem` | `59277120cfbb39fd5dda7e5a36f26f735cc3f44274c3e7a8970f33210b909f3e` |

## Demo data

`docker/demo-data/` holds the upstream's synthetic demo compositions and the
International Patient Summary template its demo seeds. The patient
identifiers in them are invented for the demo; no record of a real person is
here.

## What is left out

The upstream's Maven manifest. A vendored manifest makes this repository's
dependency graph claim a Java toolchain it does not use, and the scanners
would raise advisories against versions nothing here installs.

| File | sha256 |
|---|---|
| `pom.xml` | `4447a33ca0d3321e812252e0a2bef4b454aa66ab83bb1837e88e63cd88d72dd1` |

The upstream's `Dockerfile`, `docker-compose.yml` and `.github/` are kept.
They are inert here: GitHub runs only the workflows at the repository root,
and nothing builds this tree.
