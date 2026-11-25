<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Core FHIR IG myhealth.eu.fhir.core

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `e36ccdf4ed433415c29a11abc49f422ac2abf859d00c80556ee1ea68ff95e3dd`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `662647d07455ed798d21afde5c3f4bf9b334dd0cb3257211470a24cd7c8656b3`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The shared foundation of the MyHealth@EU FHIR guides, canonical
`http://fhir.ehdsi.eu/core`: logical models refining the Xt-EHR EHDS
models with MyHealth@EU obligations, profiles derived from HL7 Europe Base
and Core, and value sets, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.core-1.0.1.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/core/package.tgz>
- Version: 1.0.1, CI build (FHIR 4.0.1, 2026-11-23)
- sha256: `5952fc7b1c1e737b42db4d5624b70d0e24ea232a01ee2c11d874f68a7d0ebe47`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The CI build https://build-fhir.ehdsi.eu/ serves, the line the family validates against by owner ruling of 2026-11-24 (#1951); not on packages.fhir.org, so a new build fails the pin loudly and scripts/checks/pin-freshness.sh reports it.

### `myhealth.eu.fhir.core-1.0.0.tgz` (committed)

- Source: <https://packages.fhir.org/myhealth.eu.fhir.core/1.0.0>
- Version: 1.0.0, Wave 9 (FHIR 4.0.1, 2026-05-04)
- sha256: `d40b9979dd971a76fc39452aee4091a5f79a9d46192ece7c28e70b4d429a9270`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version the Patient Summary, ePrescription and eDispensation, Medical Imaging Report and Vaccination guides depend on; their builds have no newer CI build.
