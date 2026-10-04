#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/no.sh
#
# Vendors the Norwegian sources of the #488 country research into
# docs/specs/no-nhn/: ten pages of Norsk helsenett's developer portal on the
# Pasientens journaldokumenter REST API (ITI-83, ITI-104, the introduction,
# the JWT to SAML specification), HelseID (the security profile, its
# cryptography requirements, DPoP, token exchange and the token endpoint)
# and document sharing for record suppliers.
#
# Each page is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh), and
# the corpus row of docs/VERSIONS.md carries the pin-set digest. The pages
# state no licence and are fetched into the git-ignored .vendor-cache/,
# never committed. They are live pages: an edit moves the hash and the
# script fails until the pin is renewed.
#
# Usage:
#   scripts/vendor/no.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require curl shasum awk
pinned_begin

portal='https://utviklerportal.nhn.no/informasjonstjenester'
pjd="$portal/kjernejournal/pasientens-journaldokumenter/rest-api/docs"
helseid="$portal/helseid"
none='No licence statement on the page'
live='A rendered page of a live portal; an edit or a re-render moves the hash.'

pin cache no-pjd-iti83.html "published 28.08.2026" \
  "$pjd/endpoints/iti83_patientidentifierxreferencequerymd" \
  561508b2d218aa9285acee82fa46c6c76fcc7e1b6cee4c09ef9cb54f641f81da "$none" "$live"
pin cache no-pjd-iti104.html "published 28.08.2026" \
  "$pjd/endpoints/iti104_patientidentityfeedmd" \
  24404205d106bec5137a1f5ec4b1dbabe39eaca60a91bcbc130b9bcd0342d4e9 "$none" "$live"
pin cache no-pjd-introduction.html "published 28.08.2026" \
  "$pjd/introduction-en-md" \
  4ec272c6e04e2eaeebaa683ac331e46d2bfd56f30b0b9000fedbf50b8b6cf7e3 "$none" "$live"
pin cache no-pjd-jwt2saml.html "published 28.08.2026" \
  "$pjd/jwt2samlspecificationmd" \
  70573bfd42ca325feae7555567ea2b678c558cf68e0c670f14083e07d5b2c3d5 "$none" "$live"
pin cache no-helseid-security-profile.html "published 04.05.2026" \
  "$helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/sikkerhetsprofil_enmd" \
  9e99c5f493e142eae7d977d428c6f66c4dbf2aa42e74ccb9e43ca61d23c8ac8d "$none" "$live"
pin cache no-helseid-crypto.html "published 05.05.2026" \
  "$helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/vedlegg/krav_til_kryptografi_enmd" \
  2161b50d32d3143549cac0bc441b5e035edde72e3a32a9a225b442f3e5a88d13 "$none" "$live"
pin cache no-helseid-dpop.html "published 15.04.2026" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/dpop/dpop_enmd" \
  930a5a5620a834907433115552e7fb43d100bf905b4fc02e02da4a0eb4038dfd "$none" "$live"
pin cache no-helseid-token-exchange.html "published 27.04.2026" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/token_exchange_enmd" \
  f11f24294f93cd51f4a78d9e8818f3d30a1454a861bf74883a1800b53be287b1 "$none" "$live"
pin cache no-helseid-token-endpoint.html "as served on 2026-10-04" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/endepunkt/token-endepunktet_enmd" \
  1cea97865390deb4c2f3815fa68ac9eb689f7872247f935f95b83711fa005632 "$none" "$live"
pin cache no-deling-av-journaldokumenter.html "as served on 2026-10-04" \
  "$portal/deling-av-journaldokumenter" \
  9adfde76dc15ba60f4269cce35403ec63e9770f4a9d389db007becf6eac0c955 "$none" "$live"
pinned_corpus no-nhn "Norwegian NHN developer portal" \
  "the Norsk helsenett developer portal pages" \
  "The Pasientens journaldokumenter REST API (PIXm ITI-83 and ITI-104 at
PIXm 3.0.4, MHD scopes, the JWT to SAML bridge), HelseID (a FAPI 2.0 based
profile, its algorithms, DPoP, RFC 8693 token exchange and the
\`client_assertion\` every grant needs) and document sharing for record
suppliers. None states a licence, so all stay in the cache and this
directory holds the provenance alone."

say "done"
