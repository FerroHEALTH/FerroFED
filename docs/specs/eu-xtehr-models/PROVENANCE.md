<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Xt-EHR EHDS Logical Information Models xtehr.eu.ehds.models

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `5e1ac4e7cc587432984cc23fe83a8bc8f35d7da53e855e805bd3663cd3ae5569`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `bf2f18eb58b8a0baa441cc688ab8f3aaca6063229b22d5f2ee88c5cad3aaf751`
- Read by: #683 (the exchange-format proxies the gateway's FHIR face and its
  testkit are held to; the component itself moved to FerroHEALTH/FerroBRIDGE
  with #813)

The logical models of the Xt-EHR joint action for the priority categories
of Regulation (EU) 2025/327 Article 14(1), canonical
`http://www.xt-ehr.eu/fhir/models`, at the version the MyHealth@EU guides
depend on, committed under CC0-1.0. The 1.0.0 release the interoperability
component reads is vendored with it in FerroHEALTH/FerroBRIDGE (#813).

## Artefacts

### `xtehr.eu.ehds.models-0.3.0.tgz` (committed)

- Source: <https://packages.fhir.org/xtehr.eu.ehds.models/0.3.0>
- Version: 0.3.0 (FHIR 5.0.0, 2026-02-06)
- sha256: `f2cb6b0b5b35cab33e9b618b38c54797ece61f5e2dda59982c952ecd9398865b`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version every MyHealth@EU guide of scripts/vendor/myhealth-eu.sh depends on.
