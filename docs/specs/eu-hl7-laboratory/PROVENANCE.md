<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 Europe Laboratory Report guide hl7.fhir.eu.laboratory

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `ee502f9693d6df00d1b52ef7e0548eafd3512d9698d05469681d5e5635573e02`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `a2cef52c5c6d838a39a445890791e87c785094d9973ceb555404ea3c338b1bd8`
- Read by: #683 (the exchange-format proxies the gateway's FHIR face and its
  testkit are held to; the component itself moved to FerroHEALTH/FerroBRIDGE
  with #813)

The European laboratory report guide, canonical
`http://hl7.eu/fhir/laboratory`, at its current release and at the
version the MyHealth@EU guides and the interoperability component read,
committed under CC0-1.0.

## Artefacts

### `hl7.fhir.eu.laboratory-2.0.0.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.laboratory/2.0.0>
- Version: 2.0.0 (FHIR 4.0.1, 2026-05-05)
- sha256: `097aa45c6efcdbc3f25ead8b7d448fee5011b5e7ecf264aa400c5e2c69c5c488`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.

### `hl7.fhir.eu.laboratory-2.1.0.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.laboratory/2.1.0>
- Version: 2.1.0 (FHIR 4.0.1, 2026-10-16)
- sha256: `87019c396f4cff934ec5230ce1f97d76b79b40591be08161e694ad661129379c`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The current release; 2.0.0 is the version MyHealth@EU Laboratory 9.1.1 and Hospital Discharge Report 9.1.1 depend on, and the one the interoperability component reads.
