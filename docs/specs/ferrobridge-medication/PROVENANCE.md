<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: FerroBRIDGE's medication template

Vendored by `scripts/vendor/ferrobridge-medication.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `ec1c18898a60ea2d9082c509933a45c6a9d37558fb409f317efd2d1d18d511b5`
- Fetched: 2026-11-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 6 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 6 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `000c91d2d366799137306d3995109682accbf5c6b3bcbc8122ddbfe49646d91c`
- Read by: #1560 (the map mode of the prescription face,
  app/ferrofed-eehrxf)

FerroBRIDGE's ADL 2 medication template,
`openEHR-EHR-COMPOSITION.ferrobridge_medication.v1.0.0`: an encounter
`COMPOSITION` holding one `openEHR-EHR-INSTRUCTION.medication_order.v3`
and one `openEHR-EHR-ACTION.medication.v1`, whose template overlays fill
the medication, therapeutic direction, dosage and daily timing slots. The
HL7 Europe MPD project the `eehrxf` crate ships (`mappings::MPD`) maps
over this template. Beside it are the five published archetypes of the
openEHR international Clinical Knowledge Manager it specialises that
`docs/specs/openehr-ckm-medication/` does not hold, from the CKM mirror
<https://github.com/openEHR/CKM-mirror>; the other five come from there.
The template is the Licensor's own, under BUSL-1.1. Each archetype declares
the Creative Commons Attribution-ShareAlike 4.0 International License in its
own `licence` field, with the openEHR Foundation as custodian, and is
committed verbatim with attribution, as CC BY-SA permits.

## Artefacts

### `ferrobridge_medication.adlt` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/94f2f95381e96790a7f55ca44a1ca7c150e4638d/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_medication.adlt>
- Version: FerroHEALTH/FerroBRIDGE commit 94f2f95, template openEHR-EHR-COMPOSITION.ferrobridge_medication.v1.0.0
- sha256: `7d2224c9cf5b92093096c15286ff56594e88270fc23705caefceeb71a7929f75`
- Licence: BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file's own SPDX header)
- Note: The ADL 2 template with its template overlays.

### `openEHR-EHR-COMPOSITION.encounter.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/composition/openEHR-EHR-COMPOSITION.encounter.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.0.12, PUBLISHED
- sha256: `f334e2d3b7238c634cf753d19ab6e7900e8ffc4baa4b0c9614be4d55168d866c`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-CLUSTER.medication.v2.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/cluster/openEHR-EHR-CLUSTER.medication.v2.adl>
- Version: CKM mirror commit c3fdc43, revision 2.0.6, PUBLISHED
- sha256: `8ea80b21e16b8f7dbbd7d9215b55619bf5c3670813ffa3cd07198745cb9a5bb9`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/cluster/openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.4.2, PUBLISHED
- sha256: `d507ee1d0458c6b3089c9108c66a20e33dc78eb437f002e6bf69855ce406e86a`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-CLUSTER.dosage.v2.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/cluster/openEHR-EHR-CLUSTER.dosage.v2.adl>
- Version: CKM mirror commit c3fdc43, revision 2.0.3, PUBLISHED
- sha256: `5b497db6b93eceffe4589d01c8a2771c6dd00fa3a3768b11e2a4a94d99982db9`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4.

### `openEHR-EHR-CLUSTER.timing_daily.v1.adl` (committed)

- Source: <https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes/cluster/openEHR-EHR-CLUSTER.timing_daily.v1.adl>
- Version: CKM mirror commit c3fdc43, revision 1.0.2, PUBLISHED
- sha256: `b2ac788c50c76dcf641f59c8dcf75874557df8640712631fcc988197ba39605b`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4.
