<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the HL7 Europe Imaging Report R4 guide hl7.fhir.eu.imaging

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `61b070162691604c38ec749df275cffd12b5636fb1f22cf478f0eedb6500595d`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `a5850510f139fb5f714ce1e5423802fee9f1697400d58272c3e0fb33d3523f0a`
- Read by: #824 (the wire form of the Article 14(1) categories, decision A81,
  which the access record of ehds-logging writes from FerroHEALTH/FerroAUDIT
  since #1176), and the eehrxf tests and the testkit

The European imaging report guide in its ballot version and in the
1.0.0-alpha preview MyHealth@EU Medical Imaging Report 9.1.1 depends on,
canonical `http://hl7.eu/fhir/imaging`, committed under CC0-1.0.

## Artefacts

### `hl7.fhir.eu.imaging-1.0.0-ballot.tgz` (committed)

- Source: <https://packages.fhir.org/hl7.fhir.eu.imaging/1.0.0-ballot>
- Version: 1.0.0-ballot (FHIR 4.0.1, 2026-03-16)
- sha256: `bb6012bd7a019ac82f5e939dd6e892e6db6b15fd2931dcf7cc445c1ad92c3d70`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. Its Composition and DiagnosticReport profiles require the priority category code Medical-Imaging.

### `hl7.fhir.eu.imaging-1.0.0-alpha.tgz` (committed)

- Source: <https://hl7.eu/fhir/imaging/xtehr/package.tgz>
- Version: 1.0.0-alpha (FHIR 4.0.1, 2026-02-27)
- sha256: `bbc369b9aadabd96b223f4944643a9adff0c40479c61970153661a50bba73e89`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The tarball HL7 Europe serves at the path its package-list.json gives for 1.0.0-alpha, which the FHIR package registry does not list; the version myhealth.eu.fhir.imaging 9.1.1 depends on.
