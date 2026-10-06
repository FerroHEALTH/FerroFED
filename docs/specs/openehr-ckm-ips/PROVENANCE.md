<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR International Patient Summary template

Vendored by `scripts/vendor/openehr-ckm.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `ac31d1254c60a728686eb84e9dd78084de8b05ea9082342c7ed5404b7843f736`
- Fetched: 2026-10-06, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 3 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 3 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `324a035fb648bbe65eb9bd000a62e8d0256aead5e9ce0d8f5d1c7a1ef80039f4`
- Read by: #776 (the stored section queries of the patient summary,
  app/ferrofed-eehrxf)

The International Patient Summary template of the openEHR international
Clinical Knowledge Manager, <https://ckm.openehr.org/ckm/>, template
`937fca6c-ec24-4c0f-8986-623843b6ebca`, CKM cid `1013.26.376`, at asset
version 1 and status DRAFT. Its template source names one
`openEHR-EHR-SECTION.adhoc.v1` per International Patient Summary section and,
in each, the archetypes that section is recorded in. The openEHR Foundation
licenses archetypes and templates hosted in CKM under CC BY-SA
(<https://openehr.org/governance/#licensing>, "Clinical models"), and each
archetype the template names declares the Creative Commons
Attribution-ShareAlike 4.0 International License in its own `licence`
field; the template's own `licence` field is empty. Committed verbatim with
attribution, as CC BY-SA permits.

## Artefacts

### `international-patient-summary.oet` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.376/oet>
- Version: CKM cid 1013.26.376, asset version 1, DRAFT (2020-08-18)
- sha256: `e293bc003fc787e95318be0e18f32ae7ca8903ef8475ed7644b86258a59e5a34`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: The template source as CKM serves it.

### `international-patient-summary.opt` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.376/opt>
- Version: CKM cid 1013.26.376, asset version 1, DRAFT (2020-08-18)
- sha256: `981c66aa2b30e7ef686ab02859f65da773c1d97fbe97adc64935e15ebf1b08ad`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: The operational template CKM generates from the source.

### `international-patient-summary.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.376>
- Version: CKM cid 1013.26.376, asset version 1, DRAFT (2020-08-18)
- sha256: `83f22fcd2a2cd71e0097ec14f300bd4c5bec2eb139249f9aac9498942e32e8bc`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: CKM's record of the template: its id, status, asset version and project.
