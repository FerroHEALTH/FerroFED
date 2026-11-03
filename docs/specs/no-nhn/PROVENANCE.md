<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Norsk helsenett developer portal pages

Vendored by `scripts/vendor/no.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `b58525d77fc285555f92b08e592e9ccd9a4a5f96faacaf2cf7dc878c4358dfd8`
- Fetched: 2026-11-03, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 10 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The Pasientens journaldokumenter REST API (PIXm ITI-83 and ITI-104 at
PIXm 3.0.4, MHD scopes, the JWT to SAML bridge), HelseID (a FAPI 2.0 based
profile, its algorithms, DPoP, RFC 8693 token exchange and the
`client_assertion` every grant needs) and document sharing for record
suppliers. None states a licence, so all stay in the cache and this
directory holds the provenance alone.

## Artefacts

### `no-pjd-iti83.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/endpoints/iti83_patientidentifierxreferencequerymd>
- Version: published 28.08.2026
- sha256: `7691a9369a6d796292d424c44de2499d46284a4b9e09da91d6dac7a281caa1de`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-iti104.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/endpoints/iti104_patientidentityfeedmd>
- Version: published 28.08.2026
- sha256: `d983a34567af97c218d13b94fb661bb9886b95484ae01621b98259cdd7e3e499`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-introduction.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/introduction-en-md>
- Version: published 28.08.2026
- sha256: `b23920305cfd37191216bd6456484bfa93c494437161b4f10743ffd71f305aa8`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-jwt2saml.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/jwt2samlspecificationmd>
- Version: published 28.08.2026
- sha256: `e23a5e33842ee11145b9f9713a83e4ccb2c484feb2b21a9db4abbce51502de6e`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-security-profile.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/sikkerhetsprofil_enmd>
- Version: published 04.05.2026
- sha256: `f24c660231d777dcb2db87a8857c0a3ae9fb6a0788cb3ebe0c9ce0beeb48f268`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-crypto.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/vedlegg/krav_til_kryptografi_enmd>
- Version: published 05.05.2026
- sha256: `9c08a105c3a74ec0bd3dca1b302e09c62835f2f80b07fbc615f4b7aca7532c74`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-dpop.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/dpop/dpop_enmd>
- Version: published 15.04.2026
- sha256: `f7a9b43eb579ddde7fa5fab434a032dc179c5541330a10428499b063a114f213`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-token-exchange.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/token_exchange_enmd>
- Version: published 27.04.2026
- sha256: `e5d0e6b76704972fc6467566b43bf3af595de3ed12c01762ff00e19b892aa87f`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-token-endpoint.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/endepunkt/token-endepunktet_enmd>
- Version: as served on 2026-11-03
- sha256: `9263611dabacb3ef1030764d506120cf458beefe7da76a4b39ba741f495edd50`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-deling-av-journaldokumenter.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/deling-av-journaldokumenter>
- Version: as served on 2026-11-03
- sha256: `4ffb644138bc95d301eea00f03403c7b70e9f5a1809a4874a40256b38d1ecf14`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.
