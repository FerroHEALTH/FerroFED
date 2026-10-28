<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 Europe Hospital Discharge Report guide hl7.fhir.eu.hdr

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `00575e0cccddc95fb3af7edf5c1d8fa33cc86dcc3bab87874eecebdda54bc168`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `70756ebe44d75c56f0214ca1a4dfc1297ecaecd9c28f9a61790b24e5186e1fe9`
- Read by: #824 (the wire form of the Article 14(1) categories, decision A81,
  which the access record of ehds-logging writes from FerroHEALTH/FerroAUDIT
  since #1176), and the eehrxf tests and the testkit

The European hospital discharge report guide at its first release and in
the ballot version the access record's categories were read against,
canonical `http://hl7.eu/fhir/hdr`, committed under CC0-1.0.

## Artefacts

### `hl7.fhir.eu.hdr-0.1.0-ballot.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.hdr/0.1.0-ballot>
- Version: 0.1.0-ballot (FHIR 4.0.1, 2025-06-03)
- sha256: `458b8ba0edcf22d75d455d6149c9264d486bb8ec052251a90afd8654410482ee`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. It carries no priority category code.

### `hl7.fhir.eu.hdr-1.0.0.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.hdr/1.0.0>
- Version: 1.0.0 (FHIR 4.0.1, 2026-10-16)
- sha256: `eedf9af9f66b68cd66f7d6c678a36dfba86b0920df4ef15e8d2da900c4134879`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The first release; MyHealth@EU Hospital Discharge Report 9.1.1 derives from 0.1.0-ballot.
