<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR CKM medication archetypes

Vendored by `scripts/vendor/openehr-ckm.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `2658e41b88734d4cf6bc30dd2a28e91de18a067804ba43049f98d4714ad28aca`
- Fetched: 2026-10-15, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 6 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 6 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `404fc1ad09ff764a2f9ee5629fcd0adf9f31c51fb057e33583bd6b5387ec2c18`
- Read by: #980 (the openEHR mapping of ePrescriptions and eDispensations,
  #523)

Two archetypes of the openEHR international Clinical Knowledge Manager,
<https://ckm.openehr.org/ckm/>, both PUBLISHED in the project `Medication
family of archetypes` (CKM cid `1013.30.27`):
`openEHR-EHR-INSTRUCTION.medication_order.v3` (Medication order, CKM cid
`1013.1.5946`, revision 3.2.1, asset version 7) and
`openEHR-EHR-ACTION.medication.v1` (Medication management, CKM cid
`1013.1.123`, revision 1.6.0, asset version 44). Each is committed in
ADL 1.4, in the archetype XML CKM generates from it, and as CKM's record of
it. CKM serves each at an unversioned URL and ignores a version query, so
the pins hold the asset versions above and a later one fails the sha256
check. Each archetype declares the Creative Commons Attribution-ShareAlike
4.0 International License in its own `licence` field, with the openEHR
Foundation as custodian organisation and copyright holder, and the openEHR
Foundation licenses the clinical models CKM hosts under CC BY-SA
(<https://openehr.org/governance/#licensing>, "Clinical models").
Committed verbatim with attribution, as CC BY-SA permits.

## Artefacts

### `openEHR-EHR-INSTRUCTION.medication_order.v3.adl` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.5946/adl>
- Version: CKM cid 1013.1.5946, revision 3.2.1, asset version 7, PUBLISHED
- sha256: `206c2a07fa1a7197ebc9ed7a29a418ecfc9cb6b63fa2aaf811e780ee69df9e9b`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4 as CKM serves it.

### `openEHR-EHR-INSTRUCTION.medication_order.v3.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.5946/xml>
- Version: CKM cid 1013.1.5946, revision 3.2.1, asset version 7, PUBLISHED
- sha256: `e9bc195b525d4dccc772b27c69c910687c967d147f34f092606cc82124526cc6`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype XML CKM generates from the ADL.

### `openEHR-EHR-INSTRUCTION.medication_order.v3.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.5946>
- Version: CKM cid 1013.1.5946, revision 3.2.1, asset version 7, PUBLISHED
- sha256: `c5dbf4f8d89d8da4c1cba0321801d5db345c5293b3e9483db1602bbefb6abb00`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: CKM's record of the archetype: its id, status, revision and asset version.

### `openEHR-EHR-ACTION.medication.v1.adl` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.123/adl>
- Version: CKM cid 1013.1.123, revision 1.6.0, asset version 44, PUBLISHED
- sha256: `7e2484d4aec5ad8c5ceb5edb466a82b43b45396fbe3abaa4ee4a298b8e6437bc`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4 as CKM serves it.

### `openEHR-EHR-ACTION.medication.v1.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.123/xml>
- Version: CKM cid 1013.1.123, revision 1.6.0, asset version 44, PUBLISHED
- sha256: `68f3f8f7f656052a9573ee790abde1a999bf25956bd84271585b2bb0d23b3ff2`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype XML CKM generates from the ADL.

### `openEHR-EHR-ACTION.medication.v1.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.123>
- Version: CKM cid 1013.1.123, revision 1.6.0, asset version 44, PUBLISHED
- sha256: `8013573068a7a3f44fa6787c1c1a2242e774042d5bfcc20d8d78e326fee25761`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: CKM's record of the archetype: its id, status, revision and asset version.
