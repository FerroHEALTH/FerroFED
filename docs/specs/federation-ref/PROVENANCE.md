<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier reference implementation

The whole repository tree, vendored verbatim by
`scripts/vendor/federation-ref.sh`, less the dependency manifest listed
below. Never edit a file here: change the pin in docs/VERSIONS.md and re-run
the script.

- Source: <https://github.com/syntaric/openehr-federation-ref>
- Pin: commit `9cb8606d1fce457b6b7f3a42f703d7069686a413`
- Declared version: `1.0.1-SNAPSHOT` (the `pom.xml` project version, which
  tracks the specification version it implements)
- Fetched: 2026-11-02
- Upstream licence: Apache License 2.0, the repository's `LICENSE` file,
  vendored beside this file with its `NOTICE`
- Layout: the upstream paths, unchanged
- Files: 197, of which 139 are Java sources and 17 are AQL golden
  cases under `src/test/resources/aql-golden/`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `41f218888bc04914d7d49d734ef3829cd61ad33ee8a4d1c45044ed65442b0ce6`
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
| `src/test/resources/spec-schemas/federated-result-set.schema.json` | `415b6fce548810f0c86d934fe0fc7594202e913024a28c3dad9608844b47b97f` | DIFFERS from the vendored specification (`e16464c2d97248d0edeac4c48f6c694fd924bec307de60b0458be08e9774dd48`) |
| `src/test/resources/spec-schemas/options-root.schema.json` | `0ce1787028b90948057a3f53c20aa7c9d7eece70e0d968c31c0e751f1d9b5cf9` | DIFFERS from the vendored specification (`6df70649d0fd0b4795715130ec91559ab065107e8698bc60c0fc20b1e8092bed`) |

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
| `pom.xml` | `470aa82a47d477e926e10303622f3938cd066bb7141e421ed35cee0125b10a62` |

The upstream's `Dockerfile`, `docker-compose.yml` and `.github/` are kept.
They are inert here: GitHub runs only the workflows at the repository root,
and nothing builds this tree.
