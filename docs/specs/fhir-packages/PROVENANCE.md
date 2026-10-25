<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR packages of the HL7 Europe Patient Summary's dependency closure

Vendored by `scripts/vendor/fhir-packages.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `4da6e21751ced98a8fb1879aafd6b2b37b975921338adffc4d6d3465088db485`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 15 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #808 (a received document validated in full by FerroBRIDGE's
  profile validator), the testkit and the receive tests

The FHIR packages the manifests of the committed HL7 Europe packages name
and this repository does not commit, each the tarball of
`packages.fhir.org` at the exact version a manifest names. Every one
declares CC0-1.0 in its manifest except SMART App Launch, which declares
CC-BY-4.0, and the DICOM code systems, which declare `free`; they are kept
in the cache only for their size (58 MB).

## Artefacts

### `hl7.fhir.r4.core-4.0.1.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.r4.core/4.0.1>
- Version: 4.0.1
- sha256: `ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. Every package of the closure depends on it.

### `hl7.fhir.eu.base-2.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.base/2.0.0>
- Version: 2.0.0 (FHIR 4.0.1)
- sha256: `b2858677ae11245bbc768ced2e205c5354ebf71ae8d7a7a0f2915c3eb6124999`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version hl7.fhir.eu.eps 1.0.0-ballot and every MyHealth@EU guide of scripts/vendor/myhealth-eu.sh depend on; scripts/vendor/eehrxf.sh commits 2.0.1.

### `hl7.fhir.uv.extensions.r4-5.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.extensions.r4/5.2.0>
- Version: 5.2.0 (FHIR 4.0.1)
- sha256: `b406e75575f05676559d0759770c5939d023ee72fb2ef38e0b3259328487720a`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.fhir.uv.extensions.r4-5.3.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.extensions.r4/5.3.0>
- Version: 5.3.0 (FHIR 4.0.1)
- sha256: `dfbc3ac95df91ed845cc6b60920d2875f679361fa0e16f227cee93c4a9ab2104`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.fhir.uv.xver-r5.r4-0.1.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.xver-r5.r4/0.1.0>
- Version: 0.1.0 (FHIR 4.0.1)
- sha256: `7ee6f04d78ced803dd567559a0d178bafbd2d3b71db61bbbf6b15c796a1a664d`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.terminology.r4-6.2.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/6.2.0>
- Version: 6.2.0 (FHIR 4.0.1)
- sha256: `79404c9cc95491fc0155627cd039c401a6eb4748175328131e91b709a41300e2`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.terminology.r4-6.3.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/6.3.0>
- Version: 6.3.0 (FHIR 4.0.1)
- sha256: `5e080e5343406f964022b086da713df374e410245c518166c5b1b93a800cdd71`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.terminology.r4-6.5.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/6.5.0>
- Version: 6.5.0 (FHIR 4.0.1)
- sha256: `a28b638483a11df696ed92198276236d759e41c7a3a8960c9e7e7d0a1185bd06`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.terminology.r4-7.1.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.1.0>
- Version: 7.1.0 (FHIR 4.0.1)
- sha256: `1cb0cd5601972925fcd04f2c175d9cc63a9ecd9346a91a6e735f1a865dc5fba1`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.terminology.r4-7.3.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/7.3.0>
- Version: 7.3.0 (FHIR 4.0.1)
- sha256: `1a1ef2aa22ecc820341267f2bdba3b2d1f4adafb9bdddfc9e51c611cd64f3b54`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.fhir.uv.ipa-1.1.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.ipa/1.1.0>
- Version: 1.1.0 (FHIR 4.0.1)
- sha256: `013b9f91683c496fc09eed93a8798b47dcbfb775587890fa3e11253a2409feef`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.fhir.uv.smart-app-launch-2.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.smart-app-launch/2.0.0>
- Version: 2.0.0 (FHIR 4.0.1)
- sha256: `627e7a64e7abe130201fbae2ee7d2cb85d7d94972458d302c4d06d59e2c7d084`
- Licence: CC-BY-4.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `hl7.fhir.uv.ips-1.1.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.fhir.uv.ips/1.1.0>
- Version: 1.1.0 (FHIR 4.0.1)
- sha256: `403c4141101810e924f2928287985084819d8a5cc3a62e2b3840a557129840ef`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The version myhealth.eu.fhir.mvc-package 9.1.0 depends on; scripts/vendor/eehrxf.sh commits 2.0.0.

### `hl7.terminology.r4-5.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/hl7.terminology.r4/5.0.0>
- Version: 5.0.0 (FHIR 4.0.1)
- sha256: `c863bef14eddb01d2884d8f14826b9b152f9fcef6b99e107025136af69ce2c18`
- Licence: CC0-1.0: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size.

### `fhir.dicom-2022.4.20221006.tgz` (cache only, not redistributed)

- Source: <https://packages.fhir.org/fhir.dicom/2022.4.20221006>
- Version: 2022.4.20221006 (FHIR 4.0.1)
- sha256: `117a6137c7f08220a92aadaec23f10e603730fc13b583d46011a777a1f23e7ed`
- Licence: free: the license of the package manifest
- Not redistributed: the script fetches it into `.vendor-cache/fhir-packages/`, which git
  ignores; this repository carries none of its content.
- Note: The registry tarball as served, kept in the cache for its size. The DICOM code systems hl7.fhir.uv.ips 1.1.0 depends on.
