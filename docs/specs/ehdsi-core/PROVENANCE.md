<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Core FHIR IG myhealth.eu.fhir.core

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `1c03b4287f36f09497741fe240d335dd6bd1413ec4e27743427379d02b798263`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `3ef3e37f3c8712a5fd8de04c06bf93904a7656b516f52be94c23caf099611dc0`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The shared foundation of the MyHealth@EU FHIR guides, canonical
`http://fhir.ehdsi.eu/core`: logical models refining the Xt-EHR EHDS
models with MyHealth@EU obligations, profiles derived from HL7 Europe Base
and Core, and value sets, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.core-1.0.0.tgz` (committed)

- Source: <https://packages.fhir.org/myhealth.eu.fhir.core/1.0.0>
- Version: 1.0.0, Wave 9 (FHIR 4.0.1, 2026-05-04)
- sha256: `d40b9979dd971a76fc39452aee4091a5f79a9d46192ece7c28e70b4d429a9270`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. https://fhir.ehdsi.eu/core/package.tgz serves a rebuild of 1.0.0 of 2026-05-04 with other bytes, and https://build-fhir.ehdsi.eu/core/ a 1.0.1 build of 2026-10-16 (read 2026-10-24).
