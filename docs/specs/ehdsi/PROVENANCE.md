<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU NCPeH API implementation guide and OpenNCP

Vendored by `scripts/vendor/eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `653ef3161823f668756ad2b5783d29e2329926aeeac4734dee8dd9a841031598`
- Fetched: 2026-10-29, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 4 committed, 2 cache only, 0 needing
  manual retrieval
- Files in this directory: 4 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `808411588cff969914f07faf38e9d94bb03d3ada980e0bd464b4dfdaf71526c9`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes) and
  #1290 (the re-check of decisions A66 to A72), and the testkit

The FHIR package of the NCPeH API implementation guide at its current
build, 9.1.1-ci, and three of its rendered pages (the home page, Patient
Identification and Document Retrieval), all CC0-1.0; the index of the
MyHealth@EU FHIR guides, kept in the cache only; and the OpenNCP v10.1.0
source, the reference NCPeH, pinned by tag and commit and kept in the cache
as evidence only. scripts/checks/pin-freshness.sh reads the version the
build URL serves.

## Artefacts

### `myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/ncp-api/package.tgz>
- Version: 9.1.1-ci (ci-build, draft, 2026-09-08)
- sha256: `acf655481026decd6395ffc2f66c5b0b7b3316a6aaed9520db0922d727ae8fef`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: Not on packages.fhir.org; the URL serves the current build (package.json says notForPublication: true), so a new build fails the pin loudly. https://fhir.ehdsi.eu/ncp-api/package.tgz still serves 9.1.0 of 2026-05-05 (sha256 a9284da494a0402758d912791cac8aa17270bfb27983e9e21e1be23b061817ad, read 2026-10-24); 9.1.1-ci adds the study and image retrieval and the vaccination history. Source: https://code.europa.eu/ehdsi/hl7-fhir-ig/ncp-api.

### `fhir-ehdsi-index.html` (cache only, not redistributed)

- Source: <https://fhir.ehdsi.eu/>
- Version: as served on 2026-10-04
- sha256: `bc56387f605073238333db88a455d6db028d6bc36cba2625e83e1ae001cb2f0d`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/ehdsi/`, which git
  ignores; this repository carries none of its content.
- Note: The index of the MyHealth@EU FHIR implementation guides; a live page. Each guide it lists is vendored by scripts/vendor/myhealth-eu.sh or scripts/vendor/eehrxf.sh.

### `ehdsi-ncp-api-sequence-pat.html` (committed)

- Source: <https://build-fhir.ehdsi.eu/ncp-api/sequence-pat.html>
- Version: 9.1.1-ci build
- sha256: `6b9a60172684bdf1c1aed8c047e796c0dc3274dc77e20d4006d00803c9d04077`
- Licence: CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0
- Note: The rendered Patient Identification page of the current build; a new build fails the pin loudly.

### `ehdsi-ncp-api-index.html` (committed)

- Source: <https://build-fhir.ehdsi.eu/ncp-api/index.html>
- Version: 9.1.1-ci build
- sha256: `ebf58fcb97a7fdcf26281c4041a6dac39f6f8b843f307612d094c22ce0cbe47c`
- Licence: CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0
- Note: The rendered home page of the current build, with the guide scope (NCPeH to NCPeH only); a new build fails the pin loudly.

### `ehdsi-ncp-api-sequence-doc.html` (committed)

- Source: <https://build-fhir.ehdsi.eu/ncp-api/sequence-doc.html>
- Version: 9.1.1-ci build
- sha256: `b0e9ccec9df5a9456c271431e2cefeb8423c201ef1587bf331b0cc22940bb4cd`
- Licence: CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0
- Note: The rendered Document Retrieval page of the current build, with its implementation pre-requisites; a new build fails the pin loudly.

### `openncp-v10.1.0.tar.gz` (cache only, not redistributed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/archive/v10.1.0/ehealth-v10.1.0.tar.gz>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f (2026-09-29)
- sha256: `945dcfc7da3f9aaaa4a0aa57be7ee919be0c3f33a1796fffcffcd24f04fae499`
- Licence: Apache-2.0: the LICENSE and NOTICE files in the archive
- Not redistributed: the script fetches it into `.vendor-cache/ehdsi/`, which git
  ignores; this repository carries none of its content.
- Note: Evidence of the MyHealth@EU transport stack (XCPD, XCA, XDR, SAML assertions, SMP discovery), never an oracle and never a source of code, so it is kept out of the repository by decision.
