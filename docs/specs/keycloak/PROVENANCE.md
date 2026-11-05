<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Keycloak token exchange guide

Vendored by `scripts/vendor/keycloak.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `4b6a3f76c1312485668ff9b5b2d8d2daa865a48b4b879c1c93447f1b327157a1`
- Fetched: 2026-11-05, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 3 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 3 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `71f5d9d1ca91337774093a67349188e8c1bfb22e910876fefc02415a32fae686`
- Read by: #1615 (the onward token exchange of the Keycloak example profile)

The source of the Keycloak guide Configuring and using token exchange at
release 26.8.2, with the guide template that renders its preview notes and
the repository's Apache License 2.0. The guide is AsciiDoc with FreeMarker
directives, committed as the repository holds it; the rendered page at
https://www.keycloak.org/securing-apps/token-exchange serves the newest
release and is cited, not pinned.

## Artefacts

### `token-exchange.adoc` (committed)

- Source: <https://raw.githubusercontent.com/keycloak/keycloak/e514445f9dcdabc5bdcf67cab56c1e49655de067/docs/guides/securing-apps/token-exchange.adoc>
- Version: keycloak/keycloak tag 26.8.2, commit e514445f9dcdabc5bdcf67cab56c1e49655de067
- sha256: `99d6d660675f2d4d35cbdc0825ffab7f5aeeb39d970c580340b1d472a72d7afa`
- Licence: Apache-2.0: the LICENSE.txt of the keycloak/keycloak repository, vendored here
- Note: The source of the guide Configuring and using token exchange (https://www.keycloak.org/securing-apps/token-exchange). Its limitations list says Token exchange does not yet have support for the resource parameter, and its standard exchange examples carry the requesting client as azp. {project_name} renders as Keycloak in the community build.

### `features.adoc` (committed)

- Source: <https://raw.githubusercontent.com/keycloak/keycloak/e514445f9dcdabc5bdcf67cab56c1e49655de067/docs/guides/templates/features.adoc>
- Version: keycloak/keycloak tag 26.8.2, commit e514445f9dcdabc5bdcf67cab56c1e49655de067
- sha256: `46d92510c4e2cafc8780518f1932784d29359a1c8088a8b643bc63c044d0dc03`
- Licence: Apache-2.0: the LICENSE.txt of the keycloak/keycloak repository, vendored here
- Note: The guide template whose techpreview macro renders a feature the guide marks, such as token-exchange-delegation, as Preview and not fully supported in the community build.

### `LICENSE.txt` (committed)

- Source: <https://raw.githubusercontent.com/keycloak/keycloak/e514445f9dcdabc5bdcf67cab56c1e49655de067/LICENSE.txt>
- Version: keycloak/keycloak tag 26.8.2, commit e514445f9dcdabc5bdcf67cab56c1e49655de067
- sha256: `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`
- Licence: Apache-2.0, the licence text itself
- Note: The licence of the repository the two guide files come from.
