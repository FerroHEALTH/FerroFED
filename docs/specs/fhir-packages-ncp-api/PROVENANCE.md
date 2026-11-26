<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR packages the MyHealth@EU NCPeH API's dependency closure names beside the closures above

Vendored by `scripts/vendor/fhir-packages.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `3fa3a6808ddb1ff4a966938a2f38896a96eb0ec1a354291fdbf0dfec933326d1`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 5 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #1949 (each list entry the gateway writes, held to
  DocumentReference-minimal), and the testkit's NCPeH API closure

The FHIR packages the manifests of `myhealth.eu.fhir.ncp-api` 9.1.1-ci,
`ihe.iti.mhd` 4.2.2 and their dependencies name that no closure above nor
a committed package holds, each the tarball of `packages.fhir.org` at the
exact version a manifest names, or the newest release of the patch range it
names. The HL7 packages declare CC0-1.0 and the IHE package CC-BY-4.0; they
are kept in the cache only, as the closures above are (15 MB).

## Artefacts

### `hl7.terminology.r4-5.5.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/5.5.0>
- Version: 5.5.0 (FHIR 4.0.1)
- sha256: `0bc96d49e4892a91e2b087e4c8b90c3ba307e1a18dc32dca6967a0f10adb2e28`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-ncp-api/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version ihe.iti.mhd 4.2.2 depends on.

### `hl7.fhir.uv.extensions.r4-5.1.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.extensions.r4/5.1.0>
- Version: 5.1.0 (FHIR 4.0.1)
- sha256: `f96eec24d8ad2c043cf295acdfab46b01e0e1fcc568740aab3fd1acee4cb0456`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-ncp-api/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version ihe.iti.mhd 4.2.2 depends on.

### `ihe.formatcode.fhir-1.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/ihe.formatcode.fhir/1.2.0>
- Version: 1.2.0 (FHIR 4.0.1)
- sha256: `46a669d09c435e71296677dfbb9676d177eb74fa69e4c7a4ddb562552e050ff3`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-ncp-api/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The only release of the 1.2.x ihe.iti.mhd 4.2.2 depends on.

### `hl7.terminology.r4-5.3.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/5.3.0>
- Version: 5.3.0 (FHIR 4.0.1)
- sha256: `c5bc37a30763994de424fadfde536ee9d492aab8fa5ccdbe8eb4f2912fe3248a`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-ncp-api/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version ihe.formatcode.fhir 1.2.0 depends on.

### `hl7.fhir.uv.extensions.r4-1.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.extensions.r4/1.0.0>
- Version: 1.0.0 (FHIR 4.0.1)
- sha256: `b270eb5b0ea3d015398a5af6c86cab2718070cbb850e366ef050ce6d6eb4a9d3`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-ncp-api/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version ihe.formatcode.fhir 1.2.0 depends on.
