<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 Europe EU Health Data API hl7.fhir.eu.health-data-api

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `253ebbce321fc294ae582c3c3abb67274ad53de6578c3eb3360e1566697691ac`
- Fetched: 2026-10-06, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `23c48c902c598095f35e6dd5da76866e947060f2fccf88e55fef900d69ffd520`
- Read by: #824 (the wire form of the Article 14(1) categories in
  the access record of crates/ehds-logging)

The European health data API guide in its ballot version, canonical
`http://hl7.eu/fhir/health-data-api`, whose
`CodeSystem/eehrxf-document-priority-category-cs` codes the priority
categories of Regulation (EU) 2025/327 Article 14(1), committed under
CC0-1.0.

## Artefacts

### `hl7.fhir.eu.health-data-api-1.0.0-ballot.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.health-data-api/1.0.0-ballot>
- Version: 1.0.0-ballot (FHIR 4.0.1, 2026-03-13)
- sha256: `5204af60bf7f74f301a627e641a34683d392014f53d5df8ba3651c964e92977b`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. It defines EEHRxFDocumentPriorityCategoryCS, the code system of the six priority categories.
