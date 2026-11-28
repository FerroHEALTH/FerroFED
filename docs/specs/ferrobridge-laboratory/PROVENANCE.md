<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: FerroBRIDGE's laboratory report template

Vendored by `scripts/vendor/ferrobridge-laboratory.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `ea50f04e67a2a132d2fe16ee4dca1b098f53654c902dc2dcb8c8748007e560fa`
- Fetched: 2026-11-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `267af958b2d5333b82fa7f44ef0bdf5dbc71d284f7e766aa8d84af89b511d24b`
- Read by: #1404 (the receive tests of a laboratory report,
  tools/ferrofed-testkit/src/laboratory.rs)

FerroBRIDGE's laboratory report operational template,
`ferrobridge_laborbericht.v1.0.0`: the published `KDS_Laborbericht`
operational template with the Comment (`at0007`) of
`openEHR-EHR-COMPOSITION.report-result.v1` revision 1.1.0 kept in the
composition's `other_context`, where a received report names the elements
its mapping does not carry. The HL7 Europe Laboratory Report project the
`eehrxf` crate ships (`mappings::LABORATORY`) maps over this template, and
the crate ships the same file. It is a modified copy of the template the
openFHIR repository <https://github.com/openFHIR/openfhir> publishes, under
that repository's Apache License 2.0, whose text is committed beside it. It
is read by the test suites alone.

## Artefacts

### `ferrobridge_laborbericht.opt` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/33ca7e19dc55af6114988e89ed58a79a09f16b6a/crates/eehrxf/mappings/laboratory/ferrobridge_laborbericht.opt>
- Version: FerroHEALTH/FerroBRIDGE commit 33ca7e1 (eehrxf 0.52.0), template ferrobridge_laborbericht.v1.0.0
- sha256: `aa45515ae9962e71316b37acbd1ac36b9b310a9b291f58035c5dc2ae65ad3f6b`
- Licence: Apache-2.0: a modified copy of the KDS_Laborbericht operational template openFHIR/openfhir publishes under that licence (the PROVENANCE.md beside it in FerroBRIDGE)
- Note: The operational template the eehrxf crate ships.

### `LICENSE-Apache-2.0` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/33ca7e19dc55af6114988e89ed58a79a09f16b6a/crates/eehrxf/mappings/laboratory/LICENSE-Apache-2.0>
- Version: FerroHEALTH/FerroBRIDGE commit 33ca7e1
- sha256: `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4`
- Licence: Apache-2.0: a modified copy of the KDS_Laborbericht operational template openFHIR/openfhir publishes under that licence (the PROVENANCE.md beside it in FerroBRIDGE)
- Note: The Apache License 2.0 text shipped beside the template.
