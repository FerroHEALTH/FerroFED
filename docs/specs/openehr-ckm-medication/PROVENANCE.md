<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR CKM medication archetypes

Vendored by `scripts/vendor/openehr-ckm.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `d6435a63b0d59b1557a939c30080ea0c212c2e058480c74cead2b5855528a655`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 15 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 15 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b3e7fec37117ad381551ae05787840f189b6d3806f0d41e303f6e11aad718b12`
- Read by: #980 and #1043 (the openEHR mapping of ePrescriptions and
  eDispensations, #523)

Five archetypes of the openEHR international Clinical Knowledge Manager,
<https://ckm.openehr.org/ckm/>, all in the project `Medication family of
archetypes` (CKM cid `1013.30.27`). Two are PUBLISHED:
`openEHR-EHR-INSTRUCTION.medication_order.v3` (Medication order, CKM cid
`1013.1.5946`, revision 3.2.1, asset version 7) and
`openEHR-EHR-ACTION.medication.v1` (Medication management, CKM cid
`1013.1.123`, revision 1.6.0, asset version 44). Three are DRAFT, each at
revision 0.0.1-alpha: `openEHR-EHR-CLUSTER.medication_authorisation.v0`
(Medication authorisation, CKM cid `1013.1.2300`, asset version 3),
`openEHR-EHR-CLUSTER.medication_order_summary.v0` (CKM cid
`1013.1.2306`, asset version 6) and
`openEHR-EHR-CLUSTER.medication_supply_amount.v0` (CKM cid `1013.1.2453`,
asset version 5). Each is committed in ADL 1.4, in the archetype XML CKM
generates from it, and as CKM's record of it. CKM serves each at an
unversioned URL and ignores a version query, so the pins hold the asset
versions above and a later one fails the sha256 check. Each archetype
declares a Creative Commons Attribution-ShareAlike licence in its own
`licence` field, version 4.0 International or, for the authorisation and
order summary clusters, 3.0, with the openEHR Foundation as copyright
holder, and the openEHR Foundation licenses the clinical models CKM hosts
under CC BY-SA (<https://openehr.org/governance/#licensing>, "Clinical
models"). Committed verbatim with attribution, as CC BY-SA permits.

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

### `openEHR-EHR-CLUSTER.medication_authorisation.v0.adl` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2300/adl>
- Version: CKM cid 1013.1.2300, revision 0.0.1-alpha, asset version 3, DRAFT
- sha256: `920f4769badd87df62af444846560e6bed940c10ed9b358c026c3c22f025567b`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4 as CKM serves it.

### `openEHR-EHR-CLUSTER.medication_authorisation.v0.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2300/xml>
- Version: CKM cid 1013.1.2300, revision 0.0.1-alpha, asset version 3, DRAFT
- sha256: `fcae6569fc7b1a075166586140f5c190d5cb4e551ee6cbea6c8ced42c90c9849`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: The archetype XML CKM generates from the ADL.

### `openEHR-EHR-CLUSTER.medication_authorisation.v0.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2300>
- Version: CKM cid 1013.1.2300, revision 0.0.1-alpha, asset version 3, DRAFT
- sha256: `6980e1246daf62dd538354056ac4748c465964c1c56497109a8c1bbc09534d59`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: CKM's record of the archetype: its id, status, revision and asset version.

### `openEHR-EHR-CLUSTER.medication_order_summary.v0.adl` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2306/adl>
- Version: CKM cid 1013.1.2306, revision 0.0.1-alpha, asset version 6, DRAFT
- sha256: `d83990ffb5fd4f2b76ab30b6b18364e1157df742abd481cbfc166b1e01c9caec`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4 as CKM serves it.

### `openEHR-EHR-CLUSTER.medication_order_summary.v0.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2306/xml>
- Version: CKM cid 1013.1.2306, revision 0.0.1-alpha, asset version 6, DRAFT
- sha256: `10d496d61bb358f2a0f914148f0202ec5ca5370b98f589c94552539a098b3044`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: The archetype XML CKM generates from the ADL.

### `openEHR-EHR-CLUSTER.medication_order_summary.v0.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2306>
- Version: CKM cid 1013.1.2306, revision 0.0.1-alpha, asset version 6, DRAFT
- sha256: `fb3cb807b0df29d0c79e24d60702182136a7f0f37a2c354ca5002fdddfdda98f`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype's licence field)
- Note: CKM's record of the archetype: its id, status, revision and asset version.

### `openEHR-EHR-CLUSTER.medication_supply_amount.v0.adl` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2453/adl>
- Version: CKM cid 1013.1.2453, revision 0.0.1-alpha, asset version 5, DRAFT
- sha256: `a5dd6492402310e3b58fa392ef2af68bd09589bc77c9b6bc21011e3232e33df4`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype in ADL 1.4 as CKM serves it.

### `openEHR-EHR-CLUSTER.medication_supply_amount.v0.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2453/xml>
- Version: CKM cid 1013.1.2453, revision 0.0.1-alpha, asset version 5, DRAFT
- sha256: `3f15e88fb2c3db5be31a6cbd3ac5347585b43e338ccbdc88aacbba06e00c4841`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: The archetype XML CKM generates from the ADL.

### `openEHR-EHR-CLUSTER.medication_supply_amount.v0.ckm.xml` (committed)

- Source: <https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2453>
- Version: CKM cid 1013.1.2453, revision 0.0.1-alpha, asset version 5, DRAFT
- sha256: `f43c204be43b35dc866062087fc275b99a2036ff844d8e145ee66849b46e5643`
- Licence: "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype's licence field)
- Note: CKM's record of the archetype: its id, status, revision and asset version.
