<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier with AQL specification source

The whole repository tree, vendored verbatim by
`scripts/vendor/federation-spec.sh`, less the dependency manifests listed
below. Never edit a file here: change the pin in docs/VERSIONS.md and re-run
the script.

- Source: <https://github.com/syntaric/openehr-federation-spec>
- Pin: commit `b4242c291b08990df718bbfaa1188e4865478e5a`, which upstream has not tagged, by the owner's exception to the tag-only pin of this specification that docs/VERSIONS.md records
- Declared version: `spec-version: '1.0.1'`, status
  `Approved by the openEHR SEC`, dated `2026-11-01` (`antora.yml`)
- Fetched: 2026-11-02
- Upstream licence: Creative Commons Zero v1.0 Universal (CC0 1.0), the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 49, of which 25 are `modules/ROOT/pages/*.adoc`
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `1650f5b2b3091fd3376889f6761f57204d5c1679dca1126c198246351a92e2d5`
- Read by: #25 (the CP and N traceability of the conformance instrument), #33 (the wire types validated against both schemas) and #17 (the re-pin to 1.0) and #1478 (the re-pin to 1.0.1)

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
| `modules/ROOT/attachments/options-root.schema.json` | `6df70649d0fd0b4795715130ec91559ab065107e8698bc60c0fc20b1e8092bed` |
| `modules/ROOT/attachments/federated-result-set.schema.json` | `e16464c2d97248d0edeac4c48f6c694fd924bec307de60b0458be08e9774dd48` |

## What is left out

The upstream's npm manifests for its Antora site build. They are not
specification content, and a vendored manifest makes this repository's
dependency graph claim an npm toolchain it does not use.

| File | sha256 |
|---|---|
| `package-lock.json` | `f2becf243409f223a6bd267af2d1f5a7ad011bc9626bccbf104cdb00f9bd9721` |
| `package.json` | `066f9a3d177b45b015faa21adc91f837d1a79df84c27ca075a2a9a21331991f3` |

## The version

The pin is the untagged commit `b4242c291b08990df718bbfaa1188e4865478e5a`, and the document at it declares
`spec-version: '1.0.1'` with status `Approved by the openEHR SEC`.
