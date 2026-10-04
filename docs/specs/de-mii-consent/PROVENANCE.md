<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Medizininformatik-Initiative consent module

Vendored by `scripts/vendor/de.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `b3b8f3c69d0f3c2441e7fa5627b074d2281739346e01be5f4f800dd090eacb0b`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The MII consent profiles, for secondary-use consent outside the
Telematikinfrastruktur; the research mentions them without reading them.
The package states no licence of its own, so it stays in the cache and this
directory holds the provenance alone.

## Artefacts

### `de.medizininformatikinitiative.kerndatensatz.consent-2026.0.0.tgz` (cache only, not redistributed)

- Source: <https://packages.simplifier.net/de.medizininformatikinitiative.kerndatensatz.consent/2026.0.0>
- Version: 2026.0.0
- sha256: `3402dffbabee2788dd9dd07c16b4077ed2f7743e7464812138a09e587a24733a`
- Licence: No package licence: package.json has no license field and its ImplementationGuide resource has no license or copyright. One resource, the CodeSystem mii-cs-consent-version-modules, states "© 2019+ TMF e. V., Charlottenstraße 42, 10117 Berlin" and "Diese Arbeit ist lizensiert unter der Creative Commons Attribution 4.0 International License", which covers that resource alone
- Not redistributed: the script fetches it into `.vendor-cache/de-mii-consent/`, which git
  ignores; this repository carries none of its content.
