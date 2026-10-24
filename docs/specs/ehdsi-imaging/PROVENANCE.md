<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Medical Imaging Report FHIR IG myhealth.eu.fhir.imaging

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `28478270f5ca1df8358480a8b18a004e141f5ed1d710975b82306e2e2c25b6d2`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `cc8a944eff6d1ccddba9cd59450634aa731f0236ee846d408344acf84c27090d`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The MyHealth@EU Medical Imaging Report service on FHIR (Wave 9), a
preliminary draft derived from the HL7 Europe Imaging Report, canonical
`http://fhir.ehdsi.eu/imaging`, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.imaging-9.1.1.tgz` (committed)

- Source: <https://fhir.ehdsi.eu/imaging/package.tgz>
- Version: 9.1.1, draft build (FHIR 4.0.1, 2026-05-05)
- sha256: `286eebc30bc4be9c95b2478152e1dce95a6f23bc31f42f4587fd846ae04a982d`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The package the MyHealth@EU site serves for its current build (package.json says notForPublication: true); not on packages.fhir.org, so a new build fails the pin loudly. Its home page calls it a preliminary draft, not intended for implementation.
