#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
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
  7691a9369a6d796292d424c44de2499d46284a4b9e09da91d6dac7a281caa1de "$none" "$live"
pin cache no-pjd-iti104.html "published 28.08.2026" \
  "$pjd/endpoints/iti104_patientidentityfeedmd" \
  d983a34567af97c218d13b94fb661bb9886b95484ae01621b98259cdd7e3e499 "$none" "$live"
pin cache no-pjd-introduction.html "published 28.08.2026" \
  "$pjd/introduction-en-md" \
  b23920305cfd37191216bd6456484bfa93c494437161b4f10743ffd71f305aa8 "$none" "$live"
pin cache no-pjd-jwt2saml.html "published 28.08.2026" \
  "$pjd/jwt2samlspecificationmd" \
  e23a5e33842ee11145b9f9713a83e4ccb2c484feb2b21a9db4abbce51502de6e "$none" "$live"
pin cache no-helseid-security-profile.html "published 04.05.2026" \
  "$helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/sikkerhetsprofil_enmd" \
  f24c660231d777dcb2db87a8857c0a3ae9fb6a0788cb3ebe0c9ce0beeb48f268 "$none" "$live"
pin cache no-helseid-crypto.html "published 05.05.2026" \
  "$helseid/protokoller-og-sikkerhetsprofil/sikkerhetsprofil/docs/vedlegg/krav_til_kryptografi_enmd" \
  9c08a105c3a74ec0bd3dca1b302e09c62835f2f80b07fbc615f4b7aca7532c74 "$none" "$live"
pin cache no-helseid-dpop.html "published 15.04.2026" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/dpop/dpop_enmd" \
  f7a9b43eb579ddde7fa5fab434a032dc179c5541330a10428499b063a114f213 "$none" "$live"
pin cache no-helseid-token-exchange.html "published 27.04.2026" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/token_exchange_enmd" \
  e5d0e6b76704972fc6467566b43bf3af595de3ed12c01762ff00e19b892aa87f "$none" "$live"
pin cache no-helseid-token-endpoint.html "as served on 2026-11-03" \
  "$helseid/bruksmoenstre-og-eksempelkode/bruk-av-helseid/docs/teknisk-referanse/endepunkt/token-endepunktet_enmd" \
  9263611dabacb3ef1030764d506120cf458beefe7da76a4b39ba741f495edd50 "$none" "$live"
pin cache no-deling-av-journaldokumenter.html "as served on 2026-11-03" \
  "$portal/deling-av-journaldokumenter" \
  4ffb644138bc95d301eea00f03403c7b70e9f5a1809a4874a40256b38d1ecf14 "$none" "$live"
pinned_corpus no-nhn "Norwegian NHN developer portal" \
  "the Norsk helsenett developer portal pages" \
  "The Pasientens journaldokumenter REST API (PIXm ITI-83 and ITI-104 at
PIXm 3.0.4, MHD scopes, the JWT to SAML bridge), HelseID (a FAPI 2.0 based
profile, its algorithms, DPoP, RFC 8693 token exchange and the
\`client_assertion\` every grant needs) and document sharing for record
suppliers. None states a licence, so all stay in the cache and this
directory holds the provenance alone."

say "done"
