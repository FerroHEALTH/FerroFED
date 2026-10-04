<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the EPR central services interface documentation

Vendored by `scripts/vendor/ch.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `6f6eb0c51aa0843a4332e915b6f9e0020ab36d6d6dfa245eb3ff9de9a1020ed0`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The interface pack of the EPR central services (the CPI and HPD
attributes, the WSDL and the LDIF), as eHealth Suisse publishes it. It states
no licence, so it stays in the cache and this directory holds the provenance
alone.

## Artefacts

### `Central-Services_20260601_PROD.zip` (cache only, not redistributed)

- Source: <https://www.e-health-suisse.ch/payload/api/documents/file/Central-Services_20260601_PROD.zip?prefix=documents>
- Version: 20260601 PROD
- sha256: `1abcf40206643d0e7dfd85c758ca20c55b4f621c50d551891106ca89f43e10b7`
- Licence: No licence statement: neither the archive (an interface PDF, two attribute spreadsheets, WSDL.zip and LDIF.zip) nor its download names one
- Not redistributed: the script fetches it into `.vendor-cache/ch-ehs-central-services/`, which git
  ignores; this repository carries none of its content.
