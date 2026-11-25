<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Master Value Sets Catalogue myhealth.eu.fhir.mvc-package

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `bc8d5da2e3464963367980460107d58ac7faf998e66d7210497bff63155671c7`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `478a5fe2a78ec9a80e6e08dfd83dc14e24359b0a3f0b2a9eb3ddb780f20d95c6`
- Read by: #824 (the wire form of the Article 14(1) categories, decision A81,
  which the access record of ehds-logging writes from FerroHEALTH/FerroAUDIT
  since #1176), and the eehrxf tests and the testkit

The value sets MyHealth@EU exchanges are coded with, canonical
`http://fhir.ehdsi.eu/mvc-package`, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.mvc-package-9.1.0.tgz` (committed)

- Source: <https://packages.fhir.org/myhealth.eu.fhir.mvc-package/9.1.0>
- Version: 9.1.0 (FHIR 4.0.1, 2026-05-08)
- sha256: `3f99350f36a2a1ea17b7fde37c39ff766bd5c3bc9a29c6183af1acfd07b78ce1`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version MyHealth@EU Core 1.0.1, Laboratory Report 9.2.0, Hospital Discharge Report 9.2.0 and the NCPeH API 9.1.1-ci depend on. Its document type value sets code each MyHealth@EU service by LOINC, with no category code system.

### `myhealth.eu.fhir.mvc-package-10.1.0.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/mvc-package/package.tgz>
- Version: 10.1.0, CI build (FHIR 4.0.1, 2026-10-29)
- sha256: `8f42b6d0e3a5bad3517a7fe01025b1112d80b12503a66fa8495f9806a81de0e4`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The CI build https://build-fhir.ehdsi.eu/ serves, the line the family tracks by owner ruling of 2026-11-24 (#1951); not on packages.fhir.org, so a new build fails the pin loudly and scripts/checks/pin-freshness.sh reports it.
