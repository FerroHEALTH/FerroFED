<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: FerroBRIDGE's discharge report template

Vendored by `scripts/vendor/ferrobridge-discharge.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `17811510292ad89df2a37c5ab38b32f6c2439f2c65d7ca5106052ac0dc12a318`
- Fetched: 2026-11-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 5 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 5 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `5fc91a64e5bad2a7942c4571c91e2e97f397874b9f40c9eb51fbd6ffa625311b`
- Read by: #1405 (the discharge reports the FHIR face writes from a member's
  record), app/ferrofed-eehrxf and app/ferrofed-server

FerroBRIDGE's ADL 2 discharge report template,
`openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.1.0`: a transfer
of care summary `COMPOSITION` whose context is the hospital stay, holding
three ad hoc `SECTION`s, a diagnostic summary with
`openEHR-EHR-EVALUATION.problem_diagnosis.v1`, significant procedures with
`openEHR-EHR-ACTION.procedure.v1` and discharge medications with
`openEHR-EHR-INSTRUCTION.medication_order.v3`. The HL7 Europe Hospital
Discharge Report project the `eehrxf` crate ships (`mappings::HDR`) maps
over this template. Beside it are the three published archetypes of the
openEHR international Clinical Knowledge Manager it adds to the medication
archetypes of `docs/specs/ferrobridge-medication/` and
`docs/specs/openehr-ckm-medication/`, from the CKM mirror
<https://github.com/openEHR/CKM-mirror>, and the mirror's LICENSE. The
template is the Licensor's own, under BUSL-1.1. Each archetype declares the
Creative Commons Attribution-ShareAlike 4.0 licence in its own `licence`
field, with the openEHR Foundation as custodian, and is committed verbatim
with attribution, as CC BY-SA permits.

## Artefacts

### `ferrobridge_discharge_report.adlt` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/3f1953aa208c0a9d81c1847f3891b42876c1a806/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_discharge_report.adlt>
- Version: FerroHEALTH/FerroBRIDGE commit 3f1953a, template openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.1.0
- sha256: `1e4392eb1ac75878b4825bea614069801234310f359b78d95a7263726949202c`
- Licence: BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file's own SPDX header)
- Note: The ADL 2 template with its template overlays.

### `openEHR-EHR-COMPOSITION.transfer_summary.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/composition/openEHR-EHR-COMPOSITION.transfer_summary.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.0.2, PUBLISHED
- sha256: `8cfb8ec37925d85c683ad5e2803e16c14bbb51a6d6cec606308c9f8b6a8856ad`
- Licence: Creative Commons Attribution-ShareAlike 4.0 (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.7.4, PUBLISHED
- sha256: `8462a3b811a440c2b01451f0da48c7d297d53b5dc4d45c4722b13321e645ce1f`
- Licence: Creative Commons Attribution-ShareAlike 4.0 (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-ACTION.procedure.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/entry/action/openEHR-EHR-ACTION.procedure.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.5.3, PUBLISHED
- sha256: `a913b4ef4121bfd0e2c4fa975205fd588c01322cb1526eae128eced1bb1ddf11`
- Licence: Creative Commons Attribution-ShareAlike 4.0 (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `LICENSE` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/LICENSE>
- Version: CKM mirror commit c3fdc43
- sha256: `57af6f95937b451248252ebb22a129d3b65136b0c34340815b60f41b87afb6c6`
- Licence: Creative Commons Attribution-ShareAlike 3.0 Unported (the mirror's LICENSE)
- Note: The licence of the CKM mirror.
