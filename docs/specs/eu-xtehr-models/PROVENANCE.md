<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Xt-EHR EHDS Logical Information Models xtehr.eu.ehds.models

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `7d4ef5d38be992ad0d72849688f235e7621f50eb0e9821bc39ab368538e046b4`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `8385f0e3cbcd9c778bbe1e4a8b9a6391be3e8a136288d774145b8f440b0d49ca`
- Read by: #683 (the exchange-format proxies the gateway's FHIR face and its
  testkit are held to; the component itself moved to FerroHEALTH/FerroBRIDGE
  with #813)

The logical models of the Xt-EHR joint action for the priority categories
of Regulation (EU) 2025/327 Article 14(1), canonical
`http://www.xt-ehr.eu/fhir/models`, at the release and at the version the
MyHealth@EU guides depend on, committed under CC0-1.0. The continuous build
of the source repository (https://xt-ehr.github.io/xt-ehr-common/) serves
0.4.0 of 2026-02-26, older than the 1.0.0 release (read 2026-10-24).

## Artefacts

### `xtehr.eu.ehds.models-1.0.0.tgz` (committed)

- Source: <https://packages.fhir.org/xtehr.eu.ehds.models/1.0.0>
- Version: 1.0.0 (FHIR 5.0.0, 2026-04-13)
- sha256: `a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.

### `xtehr.eu.ehds.models-0.3.0.tgz` (committed)

- Source: <https://packages.fhir.org/xtehr.eu.ehds.models/0.3.0>
- Version: 0.3.0 (FHIR 5.0.0, 2026-02-06)
- sha256: `f2cb6b0b5b35cab33e9b618b38c54797ece61f5e2dda59982c952ecd9398865b`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version every MyHealth@EU guide of scripts/vendor/myhealth-eu.sh depends on.
