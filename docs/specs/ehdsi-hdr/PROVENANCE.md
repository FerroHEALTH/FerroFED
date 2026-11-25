<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Hospital Discharge Report FHIR IG myhealth.eu.fhir.hdr

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `89bd71d64ad45aa1b7dad366868565027217ec8d5758b5f7792e1d9f121ceb65`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `8e16bc3c63e2193048efacac4365369ac18551d7876cb1f79c7b6c6cd694974f`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The MyHealth@EU Hospital Discharge Report service on FHIR, derived from the HL7 Europe Hospital Discharge Report, canonical
`http://fhir.ehdsi.eu/hdr`, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.hdr-9.2.0.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/hdr/package.tgz>
- Version: 9.2.0, CI build (FHIR 4.0.1, 2026-11-24)
- sha256: `f3b649348d82c94821ef201ab9513517cfc143aa30cf623093f8772f21b93f6b`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The CI build https://build-fhir.ehdsi.eu/ serves, the line the family validates against by owner ruling of 2026-11-24 (#1951); not on packages.fhir.org, so a new build fails the pin loudly and scripts/checks/pin-freshness.sh reports it.
