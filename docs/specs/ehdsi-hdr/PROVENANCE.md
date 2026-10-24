<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Hospital Discharge Report FHIR IG myhealth.eu.fhir.hdr

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `e59077df1d1e1a7ab9f0be1f01aa5437840e6915d645c2fa1de6c9bbecc05a15`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `f09c9baa2bc0a8488e20c9ee309af84fe332481109063a9d2d2d45335d3437ec`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The MyHealth@EU Hospital Discharge Report service on FHIR (Wave 9),
derived from the HL7 Europe Hospital Discharge Report, canonical
`http://fhir.ehdsi.eu/hdr`, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.hdr-9.1.1.tgz` (committed)

- Source: <https://fhir.ehdsi.eu/hdr/package.tgz>
- Version: 9.1.1, draft build (FHIR 4.0.1, 2026-05-05)
- sha256: `38eb62092c18aa269c72cdceac01d706b293a5e5389186c682c50467bf84b38c`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The package the MyHealth@EU site serves for its current build (package.json says notForPublication: true); not on packages.fhir.org, so a new build fails the pin loudly.
