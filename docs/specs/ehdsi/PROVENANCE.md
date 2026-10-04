<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU NCPeH API implementation guide and OpenNCP

Vendored by `scripts/vendor/eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `491dc60ee8b1bf8510758e0d4a62a8578c728f27c8b519a56ba40548f72c0c56`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 2 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `ef6c57b14a45763f6418d0b37995164504468b8820e79c7f954e58338e451bc0`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The FHIR package of the NCPeH API implementation guide and its Patient
Identification page, both CC0-1.0, and the OpenNCP v10.1.0 source, the
reference NCPeH, pinned by tag and commit and kept in the cache as
evidence only.

## Artefacts

### `myhealth.eu.fhir.ncp-api-9.1.0.tgz` (committed)

- Source: <https://fhir.ehdsi.eu/ncp-api/package.tgz>
- Version: 9.1.0 (ci-build, draft, 2026-05-05)
- sha256: `a9284da494a0402758d912791cac8aa17270bfb27983e9e21e1be23b061817ad`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: Not on packages.fhir.org; the URL serves the current build (package.json says notForPublication: true), so a new build fails the pin loudly. Source: https://code.europa.eu/ehdsi/ehdsi-fhir-ig (ncp-api/).

### `fhir-ehdsi-index.html` (cache only, not redistributed)

- Source: <https://fhir.ehdsi.eu/>
- Version: as served on 2026-10-04
- sha256: `bc56387f605073238333db88a455d6db028d6bc36cba2625e83e1ae001cb2f0d`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/ehdsi/`, which git
  ignores; this repository carries none of its content.
- Note: The index of the MyHealth@EU FHIR implementation guides; a live page.

### `ehdsi-ncp-api-sequence-pat.html` (committed)

- Source: <https://fhir.ehdsi.eu/ncp-api/sequence-pat.html>
- Version: 9.1.0 build
- sha256: `f7102d0bdaadf3a1b0e7dc98ace7879775e91f9c3282bbc9ccb014dac65cce00`
- Licence: CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0
- Note: The rendered Patient Identification page of the current build; a new build fails the pin loudly.

### `openncp-v10.1.0.tar.gz` (cache only, not redistributed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/archive/v10.1.0/ehealth-v10.1.0.tar.gz>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f (2026-09-29)
- sha256: `945dcfc7da3f9aaaa4a0aa57be7ee919be0c3f33a1796fffcffcd24f04fae499`
- Licence: Apache-2.0: the LICENSE and NOTICE files in the archive
- Not redistributed: the script fetches it into `.vendor-cache/ehdsi/`, which git
  ignores; this repository carries none of its content.
- Note: Evidence of the MyHealth@EU transport stack (XCPD, XCA, XDR, SAML assertions, SMP discovery), never an oracle and never a source of code, so it is kept out of the repository by decision.
