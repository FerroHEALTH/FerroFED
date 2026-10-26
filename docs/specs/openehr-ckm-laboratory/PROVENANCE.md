<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR CKM laboratory test result template

Vendored by `scripts/vendor/openehr-ckm.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `7ae45e1e0bbc1d7e9c3f6c76c5667519229207d694bcb0c28039f6ea9a761f94`
- Fetched: 2026-10-26, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 3 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 3 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `cd8ed7b95ea995b535730cd950934e5e114ff1453acc30c5eaf7c0fddb91bffe`
- Read by: #666 (the stored section query of the laboratory report,
  app/ferrofed-eehrxf)

The template "Generic lab test result example simple" of the openEHR
international Clinical Knowledge Manager, <https://ckm.openehr.org/ckm/>,
template `0ce44071-d6e5-4fa2-9a41-046f9b4aea66`, CKM cid `1013.26.408`,
in the project "Laboratory test order & results reporting" (CKM cid
`1013.30.26`), at asset version 3 and status DRAFT: CKM has not reviewed or
published it, and a later asset version fails the sha256 check. It is
rooted at `openEHR-EHR-COMPOSITION.report-result.v1` and holds
`openEHR-EHR-OBSERVATION.laboratory_test_result.v1` with
`openEHR-EHR-CLUSTER.laboratory_test_analyte.v1`. The openEHR Foundation
licenses archetypes and templates hosted in CKM under CC BY-SA
(<https://openehr.org/governance/#licensing>, "Clinical models"); the
template's own `licence` field is empty. Committed verbatim with
attribution, as CC BY-SA permits.

## Artefacts

### `generic-lab-test-result.oet` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.408/oet>
- Version: CKM cid 1013.26.408, asset version 3, DRAFT (2021-10-18)
- sha256: `3811e80486c9fa526881e1ec891ba320b92547794ffec5447618ee9cb0216ec3`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: The template source as CKM serves it.

### `generic-lab-test-result.opt` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.408/opt>
- Version: CKM cid 1013.26.408, asset version 3, DRAFT (2021-10-18)
- sha256: `544c7f87ef86262ea50144e9955019af5850f3e422bcbd20e1a1a54edbd0f325`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: The operational template CKM generates from the source.

### `generic-lab-test-result.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.408>
- Version: CKM cid 1013.26.408, asset version 3, DRAFT (2021-10-18)
- sha256: `983bf7c3251b3f4b6bb9367d8ca56ad07839ba3c347a15a8f1dc2e3831efcbb4`
- Licence: CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template's own licence field is empty
- Note: CKM's record of the template: its id, status, asset version and project.
