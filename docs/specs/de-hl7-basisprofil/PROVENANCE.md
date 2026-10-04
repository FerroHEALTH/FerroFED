<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the German base profiles de.basisprofil.r4

Vendored by `scripts/vendor/de.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `9f7dfc9072bfec1942a16b073691433575c6312cb980d576c6f7ccb41fc2d0d6`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The package that defines the KVNR identifier systems
(`http://fhir.de/sid/gkv/kvid-10`, `identifier-pkv-kvid-10`). It states no
licence, so it stays in the cache and this directory holds the provenance
alone.

## Artefacts

### `de.basisprofil.r4-1.6.0.tgz` (cache only, not redistributed)

- Source: <https://packages.simplifier.net/de.basisprofil.r4/1.6.0>
- Version: 1.6.0
- sha256: `e0f4c05f0750afeac24e6283f98dc7711fb4d3f1f352ac1b7c88f65c1a531784`
- Licence: No licence statement: package.json has no license field, its ImplementationGuide resource has no license or copyright, and the Simplifier package page states none; 53 resources carry copyright "HL7 Deutschland e.V.", 3 "GKV-Spitzenverband" and 1 "Kassenärztliche Bundesvereinigung (KBV)"
- Not redistributed: the script fetches it into `.vendor-cache/de-hl7-basisprofil/`, which git
  ignores; this repository carries none of its content.
