<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model specification source

Vendored verbatim by `scripts/vendor/openehr-rm.sh`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/openEHR/specifications-RM>, published at
  <https://specifications.openehr.org/releases/RM/Release-1.1.0/>
- Pin: tag `Release-1.1.0`, which resolves to commit `355eb63e201c6b805c44c8809c70d63f79d54f92`; the
  `manifest.json` of that commit dates release `1.1.0` `2020-09-29`
  and declares the specifications ehr STABLE, demographic STABLE, common STABLE, data_structures STABLE, data_types STABLE, support STABLE, integration STABLE, ehr_extract STABLE
- Fetched: 2026-10-05
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's `LICENSE` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: 350
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `0529db44749170016f8cd108645902823476fc2ed1017c0b5e3e8131e60d2382`
- Read by: #696 (the RM facts the follow-up routing and the A57
  classification cite: `LOCATABLE`, `archetype_details`, the change
  control classes, `DV_TEXT.mappings` and `TERM_MAPPING`)

## What is here

- `docs/<specification>/`: one directory per RM specification (common, data
  structures, data types, demographic, EHR, EHR extract, integration,
  support), the AsciiDoc chapters with their figures.
- `docs/UML/classes/`, `docs/UML/class_index.adoc`: the class definition
  tables the chapters include, one file per class.
- `docs/index.adoc`, `manifest.json`, `README.adoc`: the component index
  and its release manifest.

The rendered `.html` pages, the UML class diagrams under
`docs/UML/diagrams/` and the UML tool's project files under
`computable/` are left out. The identifier classes (`OBJECT_VERSION_ID`,
`OBJECT_REF`, `PARTY_REF`) are defined in the openEHR BASE component,
which the RM chapters cite and this tree does not carry.
