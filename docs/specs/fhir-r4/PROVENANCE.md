<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the FHIR R4 core specification pages

Vendored by `scripts/vendor/fhir-r4.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `02abc615bcfe44cb3e2552df63fa4efa5fe44ea964a1caf1e2b54de6fc14ef5b`
- Fetched: 2026-10-16, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 7 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 7 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `6e27545f0f9a56dbbddff55ae807edca408c5f258184a469032dac4f319053ea`
- Read by: #1053 (the StructureDefinition of the remaining-dispensations
  extension, crates/eehrxf/src/medication/) and #1072 (the definitions of the
  registry's FHIR form, app/ferrofed-identity/src/ihe/mcsd/definition/ and
  app/ferrofed-registry/src/, and of the access log origin system,
  app/ferrofed-server/src/access/, and the archetype-local code systems of
  crates/eehrxf/src/terminology/, which publish none)

The Extensibility (2.5.0), Defining Extensions (2.5.1), StructureDefinition,
CodeSystem, NamingSystem, Using Codes in Resources (4.1) and License and
Legal Terms (2.20) pages of HL7 FHIR Release 4, version 4.0.1, as
`hl7.org/fhir/R4/` serves them, committed under CC0-1.0, which the license
page states for the specification.

## Artefacts

### `extensibility.html` (committed)

- Source: <https://hl7.org/fhir/R4/extensibility.html>
- Version: 4.0.1, 2.5.0 Extensibility
- sha256: `3c1df2fd19b10d5f7f132976dea0691e42addfaafd0bd10295ccf8ec51269eb3`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `defining-extensions.html` (committed)

- Source: <https://hl7.org/fhir/R4/defining-extensions.html>
- Version: 4.0.1, 2.5.1 Defining Extensions
- sha256: `dfdfa215d56375ced5f69d10ec752a33ad3088f2c4885823f01b8e18e7a21021`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `structuredefinition.html` (committed)

- Source: <https://hl7.org/fhir/R4/structuredefinition.html>
- Version: 4.0.1, the StructureDefinition resource
- sha256: `35cd9ecf45464d7f0adbc189ddd014e8641b860e4db91d1eebacc971be43c596`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `codesystem.html` (committed)

- Source: <https://hl7.org/fhir/R4/codesystem.html>
- Version: 4.0.1, the CodeSystem resource
- sha256: `6bc77ed13750d1a1fe65bdf274dfcaf7ce87c07fa23bde57ad2cdb7cdcd6b83b`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `namingsystem.html` (committed)

- Source: <https://hl7.org/fhir/R4/namingsystem.html>
- Version: 4.0.1, the NamingSystem resource
- sha256: `96b3281f6f320aa0c1b74578366296719a358151fbd9552d3a9a1b66294132f9`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `terminologies.html` (committed)

- Source: <https://hl7.org/fhir/R4/terminologies.html>
- Version: 4.0.1, 4.1 Using Codes in Resources
- sha256: `cc79f7d0fed406375185cd93d310e3394ec6f157871cc57f6408c3b7dff1cf90`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)

### `license.html` (committed)

- Source: <https://hl7.org/fhir/R4/license.html>
- Version: 4.0.1, 2.20 License and Legal Terms
- sha256: `b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15`
- Licence: CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)
- Note: The licence statement of the pages beside it, vendored as their LICENSE.
