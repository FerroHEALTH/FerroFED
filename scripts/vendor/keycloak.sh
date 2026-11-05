#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/keycloak.sh
#
# Vendors the Keycloak token exchange guide the onward exchange of #1615
# reads (docs/specs/keycloak/): its AsciiDoc source at a release tag, the
# template that renders its preview notes, and the repository's licence.
# The rendered page on keycloak.org always serves the newest release, so the
# source is pinned at the tag instead, each file by URL and sha256
# (scripts/vendor/lib/pinned.sh), and committed verbatim (Apache-2.0).
# scripts/checks/pin-freshness.sh reads the tag against the newest release.
#
# Usage:
#   scripts/vendor/keycloak.sh
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

tag=26.8.2
commit=e514445f9dcdabc5bdcf67cab56c1e49655de067
raw="https://raw.githubusercontent.com/keycloak/keycloak/$commit"
version="keycloak/keycloak tag $tag, commit $commit"
apache='Apache-2.0: the LICENSE.txt of the keycloak/keycloak repository, vendored here'

pin commit token-exchange.adoc "$version" \
  "$raw/docs/guides/securing-apps/token-exchange.adoc" \
  99d6d660675f2d4d35cbdc0825ffab7f5aeeb39d970c580340b1d472a72d7afa "$apache" \
  'The source of the guide Configuring and using token exchange (https://www.keycloak.org/securing-apps/token-exchange). Its limitations list says Token exchange does not yet have support for the resource parameter, and its standard exchange examples carry the requesting client as azp. {project_name} renders as Keycloak in the community build.'
pin commit features.adoc "$version" \
  "$raw/docs/guides/templates/features.adoc" \
  46d92510c4e2cafc8780518f1932784d29359a1c8088a8b643bc63c044d0dc03 "$apache" \
  'The guide template whose techpreview macro renders a feature the guide marks, such as token-exchange-delegation, as Preview and not fully supported in the community build.'
pin commit LICENSE.txt "$version" \
  "$raw/LICENSE.txt" \
  cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30 \
  'Apache-2.0, the licence text itself' \
  'The licence of the repository the two guide files come from.'
pinned_corpus keycloak "Keycloak token exchange guide" \
  "the Keycloak token exchange guide" \
  "The source of the Keycloak guide Configuring and using token exchange at
release $tag, with the guide template that renders its preview notes and
the repository's Apache License 2.0. The guide is AsciiDoc with FreeMarker
directives, committed as the repository holds it; the rendered page at
https://www.keycloak.org/securing-apps/token-exchange serves the newest
release and is cited, not pinned." "" \
  $'#1615 (the onward token exchange of the Keycloak example profile)'

say "done"
