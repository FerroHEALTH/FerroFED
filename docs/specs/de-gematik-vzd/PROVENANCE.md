<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the gematik VZD FHIR-Directory

Vendored by `scripts/vendor/de.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `099fb9c34136d4c7de5b9b95f07e6efda0c9b667cb19e17075d431e7b65fb562`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 3 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 3 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `559bd489c0893bf2742e658be0f9510bede2771854b7bfeca3988669220fc4a7`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The national directory of the Telematikinfrastruktur: its specification
page, its FHIR package `de.gematik.fhir.directory` 1.3.0 (canonical
`https://gematik.de/fhir/directory`, FHIR 4.0.1) and the TI
`connectionType` code system. The package and the code system are committed
under the Apache-2.0 licence of their source repository; the specification
page stays in the cache.

## Artefacts

### `gemSpec_VZD_FHIR_Directory_V1.7.0.html` (cache only, not redistributed)

- Source: <https://gemspec.gematik.de/docs/gemSpec/gemSpec_VZD_FHIR_Directory/gemSpec_VZD_FHIR_Directory_V1.7.0/>
- Version: 1.7.0 (17.02.2026)
- sha256: `bd8d56eb36eeff9ee65ce5b4a5562e9914a79466dae1b0a4d2c6b53b4f0b54dd`
- Licence: "© 2026 gematik Alle Rechte vorbehalten" (page footer)
- Not redistributed: the script fetches it into `.vendor-cache/de-gematik-vzd/`, which git
  ignores; this repository carries none of its content.
- Note: Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.

### `de.gematik.fhir.directory-1.3.0.tgz` (committed)

- Source: <https://packages.simplifier.net/de.gematik.fhir.directory/1.3.0>
- Version: 1.3.0
- sha256: `c70118a218663f685524c9888bb8ecc3bd4aa05127cee59f13ceadc0898856bb`
- Licence: Apache-2.0, from its source: the package.json has no license field; the package is built from gematik/api-vzd, whose LICENSE is Apache-2.0 (vendored beside it as api-vzd-LICENSE)
- Note: The source link: src/fhir/sushi-config.yaml at api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113 declares canonical https://gematik.de/fhir/directory and version 1.3.0, the canonical and version of this package.

### `EndpointDirectoryConnectionType.fsh` (committed)

- Source: <https://raw.githubusercontent.com/gematik/api-vzd/6377cd2ad15f6bbd7f5220f92a0a1222e6429113/src/fhir/input/fsh/codesystems/EndpointDirectoryConnectionType.fsh>
- Version: gematik/api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113
- sha256: `0efa5a159ec606ea7f84d135e6ab851200b768181153dc5db3f344bb6a553694`
- Licence: Apache-2.0: the repository LICENSE, vendored beside it as api-vzd-LICENSE

### `api-vzd-LICENSE` (committed)

- Source: <https://raw.githubusercontent.com/gematik/api-vzd/6377cd2ad15f6bbd7f5220f92a0a1222e6429113/LICENSE>
- Version: gematik/api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113
- sha256: `6a09605aea14e79bdacff887bd42d5cb80ae82dbed2756c29e408589811e3f5b`
- Licence: The repository licence: "Apache License Version 2.0, January 2004"
