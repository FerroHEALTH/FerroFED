<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Agence du Numérique en Santé FHIR guides and sources

Vendored by `scripts/vendor/fr.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `26094f61487a2a7e02d324a0ae7890f38f09bc4be4b7bfb90a266e6489c40eb0`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 7 committed, 0 cache only, 1 needing
  manual retrieval
- Files in this directory: 7 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `15a9f53c3568c28e72d7d133fd40419f1f5983c2827cce1bea51807429960577`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

FR Core 2.2.0 (the INS-NIR and INS-NIA identifier systems), the Annuaire
Santé guide 1.1.0 and its API documentation, the Pro Santé Connectée transport
security guide 1.2.0 (RFC 8693 token exchange over RFC 8705 mutual TLS) with
its web-flow page, PDSm 3.1.1 and the draft PDSm for DMP in EEDS source. The
packages and the guide page are CC0-1.0 and the two repositories MIT, all
committed whole. The INSi and DMP pages sit behind a bot challenge and are
recorded for manual retrieval.

## Artefacts

### `hl7.fhir.fr.core-2.2.0.tgz` (committed)

- Source: <https://packages.simplifier.net/hl7.fhir.fr.core/2.2.0>
- Version: 2.2.0
- sha256: `085e61bf579829a6d5520c6f388d763d27ba400f9f6ca548bc88da5545ba13f9`
- Licence: CC0-1.0: the license of the package manifest
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.

### `ans.fhir.fr.annuaire-1.1.0.tgz` (committed)

- Source: <https://packages.simplifier.net/ans.fhir.fr.annuaire/1.1.0>
- Version: 1.1.0
- sha256: `e6b09d7bc012ae5cfc9a60719ed4fc1eb86d68f04618e529937ba759c69f67d0`
- Licence: CC0-1.0: the license of the package manifest
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.

### `annuaire-sante-fhir-documentation-cdb0e03.tar.gz` (committed)

- Source: <https://github.com/ansforge/annuaire-sante-fhir-documentation/archive/cdb0e03ca57811d849254c369147bcf6ee426af8.tar.gz>
- Version: commit cdb0e03ca57811d849254c369147bcf6ee426af8 (2026-09-24)
- sha256: `4976fb2b103df926aa80abbf8aa9f89ecdbe5230399ec101820eeec47bb6b606`
- Licence: MIT: the repository licence inside the archive, "MIT License, Copyright (c) 2022 Github de l'Agence du Numérique en Santé (ANS)" (LICENSE.md)
- Note: A GitHub archive fetched by commit, kept whole so its sha256 stays the pin; GitHub does not promise byte-stable archives, so a re-generated archive fails the pin loudly.

### `ans.fr.securisation-transport-1.2.0.tgz` (committed)

- Source: <https://packages.simplifier.net/ans.fr.securisation-transport/1.2.0>
- Version: 1.2.0 (2023-12-05)
- sha256: `f938422c3a304a03ac1a4263c58f1b30a4ce296ddcdf555cc5880070e8708b3a`
- Licence: CC0-1.0: the license of the package manifest
- Note: The package carries the ImplementationGuide resource and no narrative; the narrative is in the rendered guide.

### `securisation-transport-api_prosanteconnectee_web.html` (committed)

- Source: <https://interop.esante.gouv.fr/ig/securisation-transport/api_prosanteconnectee_web.html>
- Version: 1.2.0
- sha256: `3e66d869026d94562a3391386a9874251489334f0dde0d5832a31ffc53ba9759`
- Licence: CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0
- Note: Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.

### `ans.fhir.fr.pdsm-3.1.1.tgz` (committed)

- Source: <https://packages.simplifier.net/ans.fhir.fr.pdsm/3.1.1>
- Version: 3.1.1
- sha256: `33943c9842d3d7869a72028aa2356e7b3d0def52d03982110f4a1ddfdaefb416`
- Licence: CC0-1.0: the license of the package manifest
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.

### `IG-FHIR-PDSM4DMP-EEDS-00d99b5.tar.gz` (committed)

- Source: <https://github.com/ansforge/IG-FHIR-PDSM4DMP-EEDS/archive/00d99b591ae6b1f2d040a7a215ee253481d9ee18.tar.gz>
- Version: 0.1.0 ci-build, commit 00d99b591ae6b1f2d040a7a215ee253481d9ee18
- sha256: `8de3c4a82be0bb0bef2fb256ebd2e4eb3fbedc0f1237cb140cdbd15023b37c1e`
- Licence: MIT: the repository licence inside the archive, "MIT License, Copyright (c) 2022 Github de l'Agence du Numérique en Santé (ANS)" (LICENSE)
- Note: A GitHub archive fetched by commit, kept whole so its sha256 stays the pin; GitHub does not promise byte-stable archives, so a re-generated archive fails the pin loudly.

### `referentiel-ins` (needs manual retrieval)

- Source: <https://esante.gouv.fr/produits-et-services/referentiel-ins>
- Version: not retrieved
- sha256: none, nothing is fetched
- Licence: Unknown: the page could not be read
- Needs manual retrieval: the source answers an automated client with a
  login or a bot challenge, so the script fetches nothing and this
  repository carries none of its content.
- Note: The INSi teleservice, the Référentiel INS and the DMP DSFT: esante.gouv.fr and industriels.esante.gouv.fr answer automated clients with an Incapsula challenge.
