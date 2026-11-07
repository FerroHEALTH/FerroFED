<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: FerroBRIDGE's imaging report template

Vendored by `scripts/vendor/ferrobridge-imaging.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `29cc51148ce226a68d172eb9f19d79fdfeb06ed248e8d2b5b73f9bc643ef296a`
- Fetched: 2026-11-07, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 4 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 4 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d66cde2375a196644f1dd4d81798f61aad9e4590ad47c51deffa5bb76ab04c80`
- Read by: #1406 (the imaging reports the FHIR face writes from a member's
  record), app/ferrofed-eehrxf and app/ferrofed-server

FerroBRIDGE's ADL 2 imaging report template,
`openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0`: a
`COMPOSITION.report-result.v1` holding one
`openEHR-EHR-OBSERVATION.imaging_exam_result.v1`. The HL7 Europe Imaging
Report project the `eehrxf` crate ships (`mappings::IMAGING`) maps over
this template. Beside it are the two published archetypes of the openEHR
international Clinical Knowledge Manager it specialises or holds, from the
CKM mirror <https://github.com/openEHR/CKM-mirror>, and the mirror's
LICENSE. The template is the Licensor's own, under BUSL-1.1. Each archetype
declares the Creative Commons Attribution-ShareAlike 4.0 licence in its own
`licence` field, with the openEHR Foundation as custodian, and is
committed verbatim with attribution, as CC BY-SA permits.

## Artefacts

### `ferrobridge_imaging_report.adlt` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/7498b0a22f11d9c67040ecf95f35001342c774fb/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_imaging_report.adlt>
- Version: FerroHEALTH/FerroBRIDGE tag v0.0.22, template openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0
- sha256: `1479ca7061d15cf690db731b345b5f15109e918b722fe5c83a2dfa2b730c1f78`
- Licence: BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file's own SPDX header)
- Note: The ADL 2 template with its template overlays.

### `openEHR-EHR-COMPOSITION.report-result.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/composition/openEHR-EHR-COMPOSITION.report-result.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.1.0, PUBLISHED
- sha256: `c68e587256f0cb46022f5a4ccc5c74099d41a75d143a20824dc1007b19aaddf2`
- Licence: Creative Commons Attribution-ShareAlike 4.0 (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.1.3, PUBLISHED
- sha256: `a273990123f9f9bb213cab4fe91ec98b57c7127791520895550b087863e710d5`
- Licence: Creative Commons Attribution-ShareAlike 4.0 (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `LICENSE` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/LICENSE>
- Version: CKM mirror commit c3fdc43
- sha256: `57af6f95937b451248252ebb22a129d3b65136b0c34340815b60f41b87afb6c6`
- Licence: Creative Commons Attribution-ShareAlike 3.0 Unported (the mirror's LICENSE)
- Note: The licence of the CKM mirror.
