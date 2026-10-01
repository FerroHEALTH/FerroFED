<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier with AQL specification source

The whole repository tree, vendored verbatim by
`scripts/vendor/federation-spec.sh` (.claude/rules/vendored-inputs.md),
less the dependency manifests listed below. Never edit a file here: change the
pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/syntaric/openehr-federation-spec>
- Pin: commit `7162d0c760d23105d62a743bf0ad1073c45fdb85`
- Declared version: `spec-version: '0.9.0'`, status
  `Release candidate`, dated `2026-09-13` (`antora.yml`)
- Fetched: 2026-10-01
- Upstream licence: Creative Commons Zero v1.0 Universal (CC0 1.0), the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 50, of which 26 are `modules/ROOT/pages/*.adoc`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `7925f5b64180dd28ec9e4b5987b3da507a1a12f076da2ec5443786085b1cc01d`
- Read by: #25 (the CP and N traceability of the conformance instrument), #33 (the wire types validated against both schemas) and #17 (the re-pin to 1.0)

## What is here

- `modules/ROOT/pages/`: the normative body (§1 to §18), the two annexes
  (A, the IHE binding; B, the Dutch Generic Functions) and the change record.
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
| `modules/ROOT/attachments/options-root.schema.json` | `29a0dcd9fc16f50ddb9197bcc877fb071e6d85fd2b7160717f2065eb77586fd3` |
| `modules/ROOT/attachments/federated-result-set.schema.json` | `abbcdd7f1481a6febcf5a211a987873420701991c93c89d780f660ceef1a643a` |

## What is left out

The upstream's npm manifests for its Antora site build. They are not
specification content, and a vendored manifest makes this repository's
dependency graph claim an npm toolchain it does not use.

| File | sha256 |
|---|---|
| `package-lock.json` | `19e2e272b767b757ca5f2a5f808590ab68f91ae3f7dd488403f3e4a116ac1c52` |
| `package.json` | `940883fa221caa3259ee1869aed773b02c9e2428ed37336943624b4ffa788b8a` |

## The version

The pinned commit is past the `0.9.0` git tag. It carries the SEC review
amendments of 2026-09-28 (the `meta.federation` nesting, all-or-nothing as
the default completion strategy, the `node-error` endpoint status), which
change the wire contract while the document still declares
`spec-version: '0.9.0'`. The 1.0 release replaces this pin.
