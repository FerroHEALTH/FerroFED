<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the IHE Mobile access to Health Documents profile ihe.iti.mhd

Vendored by `scripts/vendor/eehrxf.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `6f75d10023f323696b2ed5219ca1e2db57df4b41a6efede3caf69139f3ffdd70`
- Fetched: 2026-10-28, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `7f8e8a8860160d032c27c182524436a0a7e33ab5e3d1acad4aa8656a87484a7e`
- Read by: #683 (the exchange-format proxies the gateway's FHIR face and its
  testkit are held to; the component itself moved to FerroHEALTH/FerroBRIDGE
  with #813)

The IHE ITI Mobile access to Health Documents profile, canonical
`https://profiles.ihe.net/ITI/MHD`, at the version the MyHealth@EU NCPeH
API depends on, committed under CC-BY-4.0, which permits verbatim
redistribution with attribution.

## Artefacts

### `ihe.iti.mhd-4.2.2.tgz` (committed)

- Source: <https://packages.fhir.org/ihe.iti.mhd/4.2.2>
- Version: 4.2.2 (FHIR 4.0.1, 2024-05-23)
- sha256: `824061df179f6ade65c29bd79dfc35f00075748fe13054c8505a6e6f3d1cd578`
- Licence: CC-BY-4.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. The version myhealth.eu.fhir.ncp-api 9.1.1-ci depends on.
