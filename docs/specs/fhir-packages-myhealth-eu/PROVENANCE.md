<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR packages the MyHealth@EU guides and the HL7 Europe re-pins name beside the Patient Summary's closure

Vendored by `scripts/vendor/fhir-packages.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `3268287edc16e49a0d28270787ec93fb8bd06ee3f0ef7f171ee437b1adfe2cee`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 5 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #1306 (the dependency closure of the MyHealth@EU guides of
  #1290), and the testkit's myhealth_eu tests

The FHIR packages the manifests of the MyHealth@EU guides, of HL7 Europe
Laboratory Report 2.1.0 and of Hospital Discharge Report 1.0.0 name that
neither the Patient Summary's closure nor a committed package holds, each
the tarball of `packages.fhir.org` at the exact version a manifest names.
Every one declares CC0-1.0 in its manifest; they are kept in the cache only
for their size (19 MB).

## Artefacts

### `hl7.terminology.r4-7.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.2.0>
- Version: 7.2.0 (FHIR 4.0.1)
- sha256: `f8c33fd30f8daf78f7496540a841e3fd20585279c0e785fb97394e44d6f24fee`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-myhealth-eu/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version the MyHealth@EU Patient Summary, ePrescription and eDispensation and Vaccination guides depend on.

### `hl7.terminology.r4-7.4.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.4.0>
- Version: 7.4.0 (FHIR 4.0.1)
- sha256: `1dc558c204f5ee1f2b96ab9a8cd83d0811475b08a4e06a7c2bcea55d1ab116ed`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-myhealth-eu/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.laboratory 2.1.0 and hl7.fhir.eu.hdr 1.0.0 depend on.

### `hl7.terminology.r4-7.5.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.5.0>
- Version: 7.5.0 (FHIR 4.0.1)
- sha256: `1da3004587602c3f5120061abc2309fef1f96668a477a9964354fa9af6fa6e60`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-myhealth-eu/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version the MyHealth@EU Core 1.0.1, Laboratory Report 9.2.0 and Hospital Discharge Report 9.2.0 CI builds depend on.

### `hl7.fhir.uv.extensions.r4-5.4.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.extensions.r4/5.4.0>
- Version: 5.4.0 (FHIR 4.0.1)
- sha256: `e93eb1ece7d14e6b3dfaa7af5ac510577957cb5fd2b61ef95cdcfbbdca81c96d`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-myhealth-eu/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version the MyHealth@EU Core 1.0.1, Laboratory Report 9.2.0 and Hospital Discharge Report 9.2.0 CI builds depend on.

### `hl7.fhir.uv.xver-r5.r4-0.0.1-snapshot-2.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.xver-r5.r4/0.0.1-snapshot-2>
- Version: 0.0.1-snapshot-2 (FHIR 4.0.1)
- sha256: `e28ec496b04c9fa681b4c4971afb4fefc56f4dc45f2680c2935b589656731946`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages-myhealth-eu/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version the MyHealth@EU NCPeH API and Medical Imaging Report guides depend on.
