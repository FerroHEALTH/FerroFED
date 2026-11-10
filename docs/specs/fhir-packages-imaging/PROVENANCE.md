<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR packages the HL7 Europe Imaging Report's dependency closure names beside the closures above

Vendored by `scripts/vendor/fhir-packages.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `2045b0c214f518834d1882c401e4ad1e88afb005e25d10331da54307818bb766`
- Fetched: 2026-11-10, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 8 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #1785 (the imaging report the gateway writes, held to
  BundleReportEuImaging), #1716, and the testkit's imaging closure

The FHIR packages the manifests of HL7 Europe Imaging Report 1.0.0-ballot,
HL7 Europe Health Data API 1.0.0-ballot and their dependencies name that
neither closure above nor a committed package holds, each the tarball of
`packages.fhir.org` at the exact version a manifest names, or the newest
release of the patch range it names. The HL7 packages declare CC0-1.0, the
IHE packages CC-BY-4.0 and the DICOM code systems `free`; they are kept in
the cache only, as the closures above are (7 MB).

## Artefacts

### `hl7.fhir.eu.base-2.0.0-ballot.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.base/2.0.0-ballot>
- Version: 2.0.0-ballot (FHIR 4.0.1)
- sha256: `8fce7bef955e58cb677721e5ce9f1c7716e45f336f2f02cd4fb76edc6922eb31`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.imaging 1.0.0-ballot and hl7.fhir.eu.health-data-api 1.0.0-ballot depend on.

### `hl7.fhir.eu.extensions.r4-1.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.extensions.r4/1.2.0>
- Version: 1.2.0 (FHIR 4.0.1)
- sha256: `74ade1e941c1972d7adaceaad54c709ff1dc3226b3d6a180ab700bf869945377`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.imaging 1.0.0-ballot and hl7.fhir.eu.base 2.0.0-ballot depend on.

### `hl7.terminology.r4-7.0.1.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.0.1>
- Version: 7.0.1 (FHIR 4.0.1)
- sha256: `7e88619af8d6ec06f0f521a64800da7d8197ceb1c451c293ec1b796efce3840e`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.base 2.0.0-ballot and hl7.fhir.eu.extensions.r4 1.2.0 depend on.

### `fhir.dicom-2025.3.20250714.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/fhir.dicom/2025.3.20250714>
- Version: 2025.3.20250714 (FHIR 4.0.1)
- sha256: `f101876ed4463ac7b814ba0e38c97cc2a8b15aecaee66916f4644946443ba303`
- Licence: free: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The DICOM code systems hl7.fhir.eu.imaging 1.0.0-ballot depends on.

### `ihe.iti.mhd-4.2.3.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/ihe.iti.mhd/4.2.3>
- Version: 4.2.3 (FHIR 4.0.1)
- sha256: `0a11808dd94c9eac87e4ee56f0f08c2e782f022df9554acf31a3bb64a2624f2e`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.imaging 1.0.0-ballot depends on, which meets the 4.2.x of hl7.fhir.eu.health-data-api 1.0.0-ballot.

### `ihe.iti.pdqm-3.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/ihe.iti.pdqm/3.2.0>
- Version: 3.2.0 (FHIR 4.0.1)
- sha256: `61e09fbee991ff7c131b6ba5474921001e07782209961f3e85cee5f3ebcaedc2`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The newest release of the 3.2.x hl7.fhir.eu.health-data-api 1.0.0-ballot depends on.

### `ihe.iti.balp-1.1.4.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/ihe.iti.balp/1.1.4>
- Version: 1.1.4 (FHIR 4.0.1)
- sha256: `be46dda3088ee9d486d7163458dc5a4d7192e8a1c587fd47e2beb70e4bcefa90`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version ihe.iti.mhd 4.2.3 depends on, which meets the 1.1.x of ihe.iti.pdqm 3.2.0.

### `ihe.formatcode.fhir-1.4.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/ihe.formatcode.fhir/1.4.0>
- Version: 1.4.0 (FHIR 4.0.1)
- sha256: `d803d12f3b51940ae44f8a72372918cc409052e072d7e22bc968de58e301a34c`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-imaging/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The only release of the 1.4.x ihe.iti.mhd 4.2.3 depends on.
