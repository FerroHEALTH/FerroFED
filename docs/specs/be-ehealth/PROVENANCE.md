<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Belgian eHealth platform cookbooks

Vendored by `scripts/vendor/be.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `62377196eaf498ce49beca04948308cc78d02a718b5ecd1471514c1567b2c025`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 13 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The cookbooks of the Metahub, consent, the opt-out of referencing, the
therapeutic link, CoBRHA, the AddressBook, the I.AM STS and I.AM Connect and
Exchange, with the consent and therapeutic-link Swagger documents and the
platform's re-use conditions. Every cookbook allows circulation or
distribution with a reference to its URL, while the re-use conditions put the
content of downloadable documents under prior approval. Until the owner
settles which governs, every artefact stays in the cache and this directory
holds the provenance alone.

## Artefacts

### `be-metahub-ws-v2-cookbook-3.1.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/5a1b31bb3e64290d11cb5fcf308860a90874b07a/metahub-ws-v2-cookbook-3-1-dd-30042024.pdf>
- Version: v3.1 (30/04/2024); the cover reads Version 2.5
- sha256: `ddea37e8dd8252f7b9e08565c798eb2cfd31653e340485a37455b2d69fbcfa91`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-consent-ws-rest-cookbook-1.2.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/53066ecc61605b00ada5c15151e2796557964bc9/consent-ws-rest-cookbook-v1-2.pdf>
- Version: v1.2 (24/03/26)
- sha256: `e6a6b349c056ff5a319faa6abc873d82e0bda360c514d602574d6c9479c9a1db`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-consent-rest-optout-1.1.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/ace71cdffadeba27c0cd2eae72389765e85ca854/ehealth-padac-consent-ws-1-1-rest-opt-out.pdf>
- Version: v1.1 (24/03/26)
- sha256: `ad0fc3de0ad9df861e5f02bd21dd63ba6660b8aa03b12b49663519c394b0e614`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-consent-swagger-1.0.json` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/ba2c0291ed5c8a6097af4b3370591e3860bb60d9/5d7e3643b4e50b14186995c98b73920a2cdd462e/consent-swagger.json>
- Version: 1.0 (18/02/20), info.version 2.1
- sha256: `3d92515a7c4fa81218a518bd0dba0dd6e2739266db96821f443411ae273f95e1`
- Licence: No licence statement in the file; the re-use conditions say "Tenzij anders vermeld, is de informatie op deze website vrij van rechten.", and the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-therlink-ws-cookbook-2.0.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/b202405bdc5e258934ca8c2065e32ebe4a4708ee/therapeutic-link-ws-cookbook-2-0.pdf>
- Version: v2.0 (20/08/25)
- sha256: `cf2dfb33aeda3c734aa95cd8c016f4169ce6a1a0ffdd86055df232e9112617ab`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-therlink-swagger-1.0.json` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/ba2c0291ed5c8a6097af4b3370591e3860bb60d9/efe11715572b2d838b73090f04dca3f69d0c4147/therlink-swagger.json>
- Version: 1.0 (18/02/20), info.version v2.1
- sha256: `f13d3adc8f8832cee29f1b242bb26be76b565f1a01d532bbe25e26ef9a87108f`
- Licence: No licence statement in the file; the re-use conditions say "Tenzij anders vermeld, is de informatie op deze website vrij van rechten.", and the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-cobrha-consultation-cookbook-1.2.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/bbce02fa6a2722f70e0ca3e79d96c74f68ca3b51/cobrha-consultation-cookbook-v1-2-dd-04072018.pdf>
- Version: v1.2 (04/07/18)
- sha256: `df67885f7c7e0d5dd1e6d108d2a1491207743ff90b02763dde3796c468cd8c93`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-cobrha-xsd-1.8.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/d296462eba0b6e09c316e58625276d489e1194f9/cobrha-xsd-v1-8-dd-04072018.pdf>
- Version: v1.8 (04/07/18)
- sha256: `555f47b843657b5f08d8c623a598c8aa0a13bc290b4a1b27ef9992aa1100dba3`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-addressbook-rest-cookbook-1.3.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/36a8fbb0eec93cd882ed27ba5dda6743ac68b888/ehealth-addressbook-rest-v1-3-cookbook.pdf>
- Version: v1.3 (24/03/26)
- sha256: `47772795ce32e82e8fdd6df3a9d9ce1b01f8d7e9476de68e7016e65f93915083`
- Licence: Cover page: "Anyone is free to distribute this document, referring to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-sts-ws-trust-cookbook-1.2.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/5f9e569e0c2436a8ed38f8fc47cd5fba93af5667/sts-ws-trust-cookbook-v1-2.pdf>
- Version: v1.2
- sha256: `0a13c6eaab1b1f5782cedbb87e09e6a7ceccec510347ecb30289c066ecc15d14`
- Licence: Cover page: "Anyone is free to distribute this document, referring to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-iam-mobile-integration-1.13.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/9a7dc4d16e58378f81f999fc3a5333448bcdec8b/iam-mobile-integration-tech-specs-v1-13.pdf>
- Version: v1.13 (10/04/2026)
- sha256: `3daa324bf4780167b242b5d7152bc820115cbc3eddd0be02917a3ff570004930`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-iam-exchange-tech-specs-1.4.pdf` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/file/cc73d96153bbd5448a56f19d925d05b1379c7f21/7b7924a2b772b28caab82b0aec53cd7b1f218f29/iam-exchange-technical-specifications-v1-4.pdf>
- Version: v1.4
- sha256: `d710af4b3a14b4ee3582b1b02ca1a97a0f79d0d44a9011de73954002e3910e72`
- Licence: Cover page: "All are free to circulate this document with reference to the URL source."; the platform's re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto' s, afbeeldingen, geluid, video' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.

### `be-ehealth-reuse-conditions.html` (cache only, not redistributed)

- Source: <https://www.ehealth.fgov.be/ehealthplatform/nl/voorwaarden-voor-het-hergebruik>
- Version: as served on 2026-10-04
- sha256: `922c71a01095e462c68e67604cc5f33bc47a816edd9dfd35511f1326e4a95955`
- Licence: The page states the platform's re-use conditions, quoted in the rows above; whether they or the cookbooks' own clauses govern is unsettled
- Not redistributed: the script fetches it into `.vendor-cache/be-ehealth/`, which git
  ignores; this repository carries none of its content.
- Note: A live page; its bytes change with every edit.
