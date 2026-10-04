<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the gematik ePA für alle specifications and OpenAPI documents

Vendored by `scripts/vendor/de.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `ec39f57443d348a3e61330a380644c13c5ba5804e11f2a416ba0429ba031114a`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 5 committed, 2 cache only, 0 needing
  manual retrieval
- Files in this directory: 5 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `03fe5e60ba788a359980bd6b590bbb2ff602f97869f412540c9efc93c3f9f02d`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The ePA für alle record system and the primary-system integration guide,
with the four ePA-Basic OpenAPI documents the research reads: the Information
Service (`getRecordStatus`, the `x-insurantid` header), Entitlement
Management, Consent Decision Management and the Authorization Service. The
OpenAPI documents and the repository licence are committed under
Apache-2.0. The two specification pages carry gematik's "Alle Rechte
vorbehalten" and stay in the cache.

## Artefacts

### `gemILF_PS_ePA_V3.8.1.html` (cache only, not redistributed)

- Source: <https://gemspec.gematik.de/docs/gemILF/gemILF_PS_ePA/gemILF_PS_ePA_V3.8.1/>
- Version: 3.8.1 (24.03.2026)
- sha256: `c03d39145f08a4eaffade5cd3a0c033d732efc1aba6d27fb62674393849388d1`
- Licence: "© 2026 gematik Alle Rechte vorbehalten" (page footer)
- Not redistributed: the script fetches it into `.vendor-cache/de-gematik-epa/`, which git
  ignores; this repository carries none of its content.
- Note: Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.

### `gemSpec_Aktensystem_ePAfueralle_V1.8.2.html` (cache only, not redistributed)

- Source: <https://gemspec.gematik.de/docs/gemSpec/gemSpec_Aktensystem_ePAfueralle/gemSpec_Aktensystem_ePAfueralle_V1.8.2/>
- Version: 1.8.2 (14.07.2026)
- sha256: `9443b45f4a8242174002fb3572d2e0ae8c924b60e39f09642ec089f6c5bd04b1`
- Licence: "© 2026 gematik Alle Rechte vorbehalten" (page footer)
- Not redistributed: the script fetches it into `.vendor-cache/de-gematik-epa/`, which git
  ignores; this repository carries none of its content.
- Note: Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.

### `I_Information_Service-ePA-3.1.3.yaml` (committed)

- Source: <https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6/src/openapi/I_Information_Service.yaml>
- Version: tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6, API 1.5.1
- sha256: `4b140f37b58d3d1819c6aaf2398b949e54217207685a87ae5a7114409dc52373`
- Licence: Apache-2.0: the document's info.license is "Apache 2.0" (https://www.apache.org/licenses/LICENSE-2.0), and the repository LICENSE is vendored beside it as ePA-Basic-LICENSE

### `I_Entitlement_Management-ePA-3.1.3.yaml` (committed)

- Source: <https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6/src/openapi/I_Entitlement_Management.yaml>
- Version: tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6, API 1.8.0
- sha256: `8422a18ff5af3a0c812dcc7e560eaeaee644a6ce4db92fed980eaf009b3b7406`
- Licence: Apache-2.0: the document's info.license is "Apache 2.0" (https://www.apache.org/licenses/LICENSE-2.0), and the repository LICENSE is vendored beside it as ePA-Basic-LICENSE

### `I_Consent_Decision_Management-ePA-3.1.3.yaml` (committed)

- Source: <https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6/src/openapi/I_Consent_Decision_Management.yaml>
- Version: tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6, API 1.7.1
- sha256: `cc6fba385c2e33b2dfbf434f695d850b785418e53e5dd8464b82a807ebc2ef59`
- Licence: Apache-2.0: the document's info.license is "Apache 2.0" (https://www.apache.org/licenses/LICENSE-2.0), and the repository LICENSE is vendored beside it as ePA-Basic-LICENSE

### `I_Authorization_Service-ePA-3.1.3.yaml` (committed)

- Source: <https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6/src/openapi/I_Authorization_Service.yaml>
- Version: tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6, API 1.9.1
- sha256: `82307d87de0a9a662c951663cbd584eff5dae8bea1820b7ff0a2129f5de0ff9b`
- Licence: Apache-2.0: the document's info.license is "Apache 2.0" (https://www.apache.org/licenses/LICENSE-2.0), and the repository LICENSE is vendored beside it as ePA-Basic-LICENSE

### `ePA-Basic-LICENSE` (committed)

- Source: <https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6/LICENSE>
- Version: tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6
- sha256: `40c467d22ec8924e0388c6936baf74a039f5f1f9be4bcd0d42b2ce3c0619765f`
- Licence: The repository licence: "Apache License Version 2.0, January 2004"
