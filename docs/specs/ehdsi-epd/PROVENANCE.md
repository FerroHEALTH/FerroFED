<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Electronic Prescription and Dispense FHIR IG myhealth.eu.fhir.epd

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `27781c1e57be144cd78fccd659f8f27eead8e70c29149dbdde0b9774a2de95d4`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b83069a9a67cc4f13b359020e098a2791977ad55321c62245585120644cb663f`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The proof of concept of the MyHealth@EU ePrescription and eDispensation
service on FHIR, derived from the HL7 Europe Medication Prescription and
Dispense guide, canonical `http://fhir.ehdsi.eu/epd`, with its rendered
home page, committed under CC-BY-4.0, with the copyright the ImplementationGuide resource carries kept
verbatim in the package.

## Artefacts

### `myhealth.eu.fhir.epd-1.0.0-poc.tgz` (committed)

- Source: <https://packages.fhir.org/myhealth.eu.fhir.epd/1.0.0-poc>
- Version: 1.0.0-poc, QA preview (FHIR 4.0.1, 2026-07-16)
- sha256: `5e9e01171e70cb62d523c64c7e47238584bdde3c43875dd09fd2dbb5eaed1d5e`
- Licence: CC-BY-4.0: the license of the package manifest and of the ImplementationGuide resource, whose copyright reads "Copyright European Union, represented by the European Commission - DG SANTE. Licenced under CC-BY-4.0."
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. https://fhir.ehdsi.eu/epd/package.tgz serves a rebuild of 1.0.0-poc with other bytes (read 2026-10-24).

### `myhealth.eu.fhir.epd-index.html` (committed)

- Source: <https://fhir.ehdsi.eu/epd/index.html>
- Version: 1.0.0-poc build
- sha256: `f3f922ce9535978fba76fc080c3b209e012f274ed26302861189bae633479b16`
- Licence: CC-BY-4.0: a page of the implementation guide whose ImplementationGuide resource declares license CC-BY-4.0 and whose footer carries the same copyright
- Note: The rendered home page, whose scope says the ePrescription and eDispensation service of MyHealth@EU runs on HL7 CDA and that the guide is a proof of concept of the same service on FHIR; a new build fails the pin loudly.
