#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/dk.sh
#
# Vendors the Danish sources of the #488 country research into
# docs/specs/dk-nsp/: five NSPOP Confluence pages on document sharing over
# the National Service Platform (DROS, DDS, the patient index), MinSpærring
# consent, the SOR organisation register and the eHDSI XUA token exchange,
# as the Confluence REST API exports them.
#
# Each page is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh), and
# the corpus row of docs/VERSIONS.md carries the pin-set digest. The pages
# state no licence and are fetched into the git-ignored .vendor-cache/,
# never committed. They are live pages: an edit moves the hash and the
# script fails until the pin is renewed.
#
# Usage:
#   scripts/vendor/dk.sh
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

api='https://www.nspop.dk/rest/api/content'
none='No licence statement on the page'
live='A Confluence REST export of a live page; each new page version moves the hash.'

pin cache dk-nspop-154772568.json "page version 72 (2026-06-30): Dokumentdeling på NSP" \
  "$api/154772568?expand=body.storage,version" \
  ec76a5193a17664f8e7ac8e216883a09fd29f6ec939632ad39fb4a374bac4688 "$none" "$live"
pin cache dk-nspop-102375271.json "page version 105 (2026-08-25): DDS - Guide til anvendere" \
  "$api/102375271?expand=body.storage,version" \
  471bedb0b8d1a61ac2ce4cf4a1fc518c6338adbaa3e476fbb8e5e856ff188d0f "$none" "$live"
pin cache dk-nspop-102389289.json "page version 5 (2023-09-20): MinSpærring - Positiv samtykke funktionalitet" \
  "$api/102389289?expand=body.storage,version" \
  e4f6854be9d627c9d9492c5a9a0c25825e99b3b951eb41d38e9421159e237b90 "$none" "$live"
pin cache dk-nspop-87377126.json "page version 81 (2025-11-27): SOR Sundhedsvæsenets Organisationsregister" \
  "$api/87377126?expand=body.storage,version" \
  8bca1964db9ad3a4d0122ba825ebaee7a700748432b278872b1d5b34760a6d61 "$none" "$live"
pin cache dk-nspop-280907107.json "page version 21 (2025-12-03): SEAL.JAVA 2 - eHDSI IDWS XUA omveksling" \
  "$api/280907107?expand=body.storage,version" \
  840d6113c3e2cac2c9640685bdc9695f3200a810aea9ebdb30bfef5ad3931e50 "$none" "$live"
pinned_corpus dk-nsp "Danish NSP documentation (NSPOP)" \
  "the Danish National Service Platform documentation" \
  "Five NSPOP pages: document sharing on the NSP, the DDS user guide (a
search needs at least a CPR number), MinSpærring positive consent, the SOR
organisation register and the eHDSI XUA token exchange at the SOSI-STS. None
states a licence, so all stay in the cache and this directory holds the
provenance alone."

say "done"
