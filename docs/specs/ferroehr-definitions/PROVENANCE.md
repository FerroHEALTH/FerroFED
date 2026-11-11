<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: FerroEHR's professional PERSON and ROLE definitions

Vendored by `scripts/vendor/ferroehr-definitions.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `1563759c01325d1a409f58558157785d991696fff9557d19675d77335cb82f66`
- Fetched: 2026-11-11, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 3 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 3 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `57b764d1ce85a03fa1ee4a60204d9e5ddc01ab54d33e0fdb18189a4cb211cc18`
- Read by: #1615 (the quickstart and the e2e nodes reached under the caller),
  compose.yaml and tools/ferrofed-testkit

FerroEHR's professional PERSON and ROLE definitions,
`openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0` and
`openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0`: the test and
quickstart pair a FerroEHR node of release 5.7.0 or later loads with
`ferroehr definitions load` before `serve`, so that its party mapping
(`[authz.party_binding.person]` and `[authz.party_binding.professional_role]`)
finds its templates at boot and every person who signs in binds to a
`PERSON`. FerroEHR wrote them by hand as adaptations of openEHR CKM
archetypes and states that they carry the Creative Commons
Attribution-ShareAlike 3.0 licence and the copyright line of their CKM
sources (© openEHR Foundation); they are committed verbatim with that
attribution and the licence text, as CC BY-SA permits.

## Artefacts

### `openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroEHR/7d85f0a1d2b86ca5bf2d43025090e6a102d99f74/deploy/definitions/professional/openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls>
- Version: FerroHEALTH/FerroEHR tag v5.7.0
- sha256: `22e0004006afb36e903841afecaa2541c7624f3f1d6e03dfeb5c9354ee370c7f`
- Licence: Creative Commons Attribution-ShareAlike 3.0, © openEHR Foundation (FerroEHR's deploy/definitions/professional/PROVENANCE.md)
- Note: The professional PERSON, loaded first.

### `openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroEHR/7d85f0a1d2b86ca5bf2d43025090e6a102d99f74/deploy/definitions/professional/openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls>
- Version: FerroHEALTH/FerroEHR tag v5.7.0
- sha256: `a9cb003e2238dd4ba8d69b5184fc917102b538b198344d9507f303215b9a3fec`
- Licence: Creative Commons Attribution-ShareAlike 3.0, © openEHR Foundation (FerroEHR's deploy/definitions/professional/PROVENANCE.md)
- Note: The professional ROLE, loaded second.

### `LICENSE-CC-BY-SA-3.0` (committed)

- Source: <https://raw.githubusercontent.com/FerroHEALTH/FerroEHR/7d85f0a1d2b86ca5bf2d43025090e6a102d99f74/LICENSE-CC-BY-SA-3.0>
- Version: FerroHEALTH/FerroEHR tag v5.7.0
- sha256: `3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f`
- Licence: Creative Commons Attribution-ShareAlike 3.0 Unported (the licence text)
- Note: The licence the two definitions carry.
