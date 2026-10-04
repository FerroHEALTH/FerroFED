<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Norsk helsenett developer portal pages

Vendored by `scripts/vendor/no.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `3eef682e5633ba560c8cbe2caee315501204cba9935a9432fdb2a4308e64364b`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
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
- sha256: `561508b2d218aa9285acee82fa46c6c76fcc7e1b6cee4c09ef9cb54f641f81da`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-iti104.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/endpoints/iti104_patientidentityfeedmd>
- Version: published 28.08.2026
- sha256: `24404205d106bec5137a1f5ec4b1dbabe39eaca60a91bcbc130b9bcd0342d4e9`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-introduction.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/introduction-en-md>
- Version: published 28.08.2026
- sha256: `4ec272c6e04e2eaeebaa683ac331e46d2bfd56f30b0b9000fedbf50b8b6cf7e3`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-pjd-jwt2saml.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/kjernejournal/pasientens-journaldokumenter/rest-api/docs/jwt2samlspecificationmd>
- Version: published 28.08.2026
- sha256: `70573bfd42ca325feae7555567ea2b678c558cf68e0c670f14083e07d5b2c3d5`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-security-profile.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/sikkerhetsprofil_enmd>
- Version: published 04.05.2026
- sha256: `9e99c5f493e142eae7d977d428c6f66c4dbf2aa42e74ccb9e43ca61d23c8ac8d`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-crypto.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/vedlegg/krav_til_kryptografi_enmd>
- Version: published 05.05.2026
- sha256: `2161b50d32d3143549cac0bc441b5e035edde72e3a32a9a225b442f3e5a88d13`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-dpop.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/dpop/dpop_enmd>
- Version: published 15.04.2026
- sha256: `930a5a5620a834907433115552e7fb43d100bf905b4fc02e02da4a0eb4038dfd`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-token-exchange.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/token_exchange_enmd>
- Version: published 27.04.2026
- sha256: `f11f24294f93cd51f4a78d9e8818f3d30a1454a861bf74883a1800b53be287b1`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-helseid-token-endpoint.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/endepunkt/token-endepunktet_enmd>
- Version: as served on 2026-10-04
- sha256: `1cea97865390deb4c2f3815fa68ac9eb689f7872247f935f95b83711fa005632`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.

### `no-deling-av-journaldokumenter.html` (cache only, not redistributed)

- Source: <https://utviklerportal.nhn.no/informasjonstjenester/deling-av-journaldokumenter>
- Version: as served on 2026-10-04
- sha256: `9adfde76dc15ba60f4269cce35403ec63e9770f4a9d389db007becf6eac0c955`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/no-nhn/`, which git
  ignores; this repository carries none of its content.
- Note: A rendered page of a live portal; an edit or a re-render moves the hash.
