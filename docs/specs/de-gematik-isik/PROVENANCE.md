<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the gematik ISiK package de.gematik.isik

Vendored by `scripts/vendor/de.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `6331afd5ea3c218941d399fcb8b11fd3541c54f678953eecaf59dd6febb509f0`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

ISiK 6.0.0, whose `ISiKPatient` carries the KVNR. The package states no
licence; the ISiK Basismodul source repository is Apache-2.0, but nothing
links it to this package, so the package stays in the cache and this
directory holds the provenance alone.

## Artefacts

### `de.gematik.isik-6.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.simplifier.net/de.gematik.isik/6.0.0>
- Version: 6.0.0
- sha256: `dc4850239599565b67f75a1daf3ee242f288f991d38b8e65c5edeb5299ee9b92`
- Licence: No licence statement: package.json has no license field (author "gematik GmbH"), the package has no ImplementationGuide resource, no resource states a licence of its own, and the Simplifier package page states none
- Not redistributed: the script fetches it into `.vendor-cache/de-gematik-isik/`, which git
  ignores; this repository carries none of its content.
