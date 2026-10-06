<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR BASE component specification source

Vendored verbatim by `scripts/vendor/openehr-base.sh`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/specifications-BASE>, published at
  <https://specifications.openehr.org/releases/BASE/Release-1.1.0/>
- Pin: tag `Release-1.1.0`, which resolves to commit `a7154147fc17721fd9d575b61081a1e54113b974`; the amendment
  record of the Base Types specification heads its entries `BASE Release
  1.1.0`, and the specifications declare the status architecture_overview STABLE, foundation_types STABLE, base_types STABLE, resource STABLE
- Paired with: the openEHR Reference Model `Release-1.1.0` vendored under
  `docs/specs/openehr-rm/`, whose chapters cite the classes defined here
- Fetched: 2026-10-06
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 206
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `41466cd8e2d454f3c1016ebef066fff25ae9fbcde64ddd2c2d4ca5fae1c51d8c`
- Read by: #702 (the identifier classes `OBJECT_VERSION_ID`,
  `OBJECT_REF` and `PARTY_REF` the follow-up routing and the access
  record cite)

## What is here

- `docs/<specification>/`: one directory per BASE specification
  (architecture overview, foundation types, base types, resource), the
  AsciiDoc chapters with their figures.
- `docs/UML/classes/`, `docs/UML/class_index.adoc`: the class definition
  tables the chapters include, one file per class.
- `docs/index.adoc`, `README.adoc`: the component index and the
  repository's description.

The rendered `.html` pages, the UML class diagrams under
`docs/UML/diagrams/`, the UML tool's project files under `computable/`,
the UML export scripts and `example/example.bmm` are left out.
