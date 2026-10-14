<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier with AQL specification source

The whole repository tree, vendored verbatim by
`scripts/vendor/federation-spec.sh`, less the dependency manifests listed
below. Never edit a file here: change the pin in docs/VERSIONS.md and re-run
the script.

- Source: <https://github.com/syntaric/openehr-federation-spec>
- Pin: tag `1.0.0`, which resolves to commit `7dcf0f44ac122fe947b1e5ceba6bcbbb7721917d`
- Declared version: `spec-version: '1.0.0'`, status
  `Approved by the openEHR SEC`, dated `2026-10-12` (`antora.yml`)
- Fetched: 2026-10-14
- Upstream licence: Creative Commons Zero v1.0 Universal (CC0 1.0), the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 49, of which 25 are `modules/ROOT/pages/*.adoc`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `de7c175163395cb10a716b735e629d2752cf4e37de05ea88b366af2924af008d`
- Read by: #25 (the CP and N traceability of the conformance instrument), #33 (the wire types validated against both schemas) and #17 (the re-pin to 1.0)

## What is here

- `modules/ROOT/pages/`: the normative body (§1 to §18) and the two
  annexes (A, the IHE binding; B, the Dutch Generic Functions).
  The N# requirements are in `requirements.adoc` and the CP# conformance
  points in `conformance.adoc`.
- `modules/ROOT/attachments/`: the two published JSON schemas.
- `diagrams/` and `modules/ROOT/images/`: the PlantUML sources and the
  rendered figures.
- `tools/`: the upstream's own reference, anchor, schema and traceability
  checks, with the requirement-to-conformance-point table
  `traceability.tsv`.
- `.github/workflows/`: the upstream's site build. Inert here, since GitHub
  runs only the workflows at the repository root.

| Schema | sha256 |
|---|---|
| `modules/ROOT/attachments/options-root.schema.json` | `0ce1787028b90948057a3f53c20aa7c9d7eece70e0d968c31c0e751f1d9b5cf9` |
| `modules/ROOT/attachments/federated-result-set.schema.json` | `415b6fce548810f0c86d934fe0fc7594202e913024a28c3dad9608844b47b97f` |

## What is left out

The upstream's npm manifests for its Antora site build. They are not
specification content, and a vendored manifest makes this repository's
dependency graph claim an npm toolchain it does not use.

| File | sha256 |
|---|---|
| `package-lock.json` | `c2984805f5e2bfb5acc4b9600d6a5d03ef57f43e5575883e6bb951bcc01c1142` |
| `package.json` | `c939ef7a21aa6cc405fbcbdb5c404d23f3d39dabeee2818873c25608e1ea9a04` |

## The version

The pin is the `1.0.0` release tag, and the document at it declares
`spec-version: '1.0.0'` with status `Approved by the openEHR SEC`.
