#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openid.sh
#
# Fetches the OpenID Foundation specifications the Annex B authentication
# tracks cite into .vendor-cache/openid/, and writes their provenance to
# docs/specs/openid/PROVENANCE.md: the FAPI 2.0 Security Profile (the
# BgZ/eOverdracht track of Annex B §B.4a), OpenID for Verifiable Credential
# Issuance 1.0 (GFI-002 of the Netherlands Generic Functions IG) and OpenID
# for Verifiable Presentations draft 18 (the vp_token encoding Nuts RFC021 §3
# names for its assertion).
#
# Redistribution: the OpenID Foundation licenses each document "solely for
# the purposes of (i) developing specifications, and (ii) implementing" it.
# A licence limited by purpose does not cover a public repository whose
# readers may have any purpose, so the documents are never committed: the
# script verifies each against the sha256 its docs/VERSIONS.md row pins and
# keeps it under .vendor-cache/, which git ignores. Only the provenance is
# committed. The script fails if a document stops carrying the OIDF notice.
#
# Usage:
#   scripts/vendor/openid.sh
#
# Requires: curl, shasum.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum

dest="docs/specs/openid"
cache=".vendor-cache/openid"
notice="The OpenID Foundation (OIDF) grants to any Contributor, developer"

# Each document: its docs/VERSIONS.md row and the file it is kept as.
documents=(
  "OpenID FAPI 2.0 Security Profile|fapi-security-profile-2_0-final.html"
  "OpenID for Verifiable Credential Issuance 1.0|openid-4-verifiable-credential-issuance-1_0-final.html"
  "OpenID for Verifiable Presentations draft 18|openid-4-verifiable-presentations-1_0-18.html"
)

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

rows=""
for entry in "${documents[@]}"; do
  item="${entry%%|*}"
  file="${entry#*|}"
  say "fetching $item"
  corpus_fetch_pinned "$item" "$tmp/$file"
  # The notice wraps differently in each rendering, so lines are joined first.
  tr '\n' ' ' < "$tmp/$file" | grep -qF "$notice" \
    || die "$item no longer carries the OpenID Foundation copyright licence"
  pin="$(corpus_pin_cell "$item")"
  rows="$rows
| $item | <$(corpus_pin_url "$pin")> | \`$(corpus_pin_sha256 "$pin")\` |"
done

rm -rf "$cache"
mkdir -p "$cache" "$dest"
for entry in "${documents[@]}"; do
  file="${entry#*|}"
  cp "$tmp/$file" "$cache/$file"
done

fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes third-party material that is pinned and fetched,
     never committed; the bytes keep their upstream licence. -->

# Provenance: the OpenID Foundation specifications

Pinned and fetched by \`scripts/vendor/openid.sh\` into \`$cache/\`, which
git ignores. Nothing but this file is committed. To refresh, change the pins in
docs/VERSIONS.md and re-run the script.

- Source: the OpenID Foundation, <https://openid.net/specs/>
- Fetched: $fetched
- Upstream licence: each document's Notices appendix, verbatim in its first
  sentence: "The OpenID Foundation (OIDF) grants to any Contributor,
  developer, implementer, or other interested party a non-exclusive, royalty
  free, worldwide copyright license to reproduce, prepare derivative works
  from, distribute, perform and display, this Implementers Draft, Final
  Specification, or Final Specification Incorporating Errata Corrections
  solely for the purposes of (i) developing specifications, and (ii)
  implementing Implementers Drafts, Final Specifications, and Final
  Specification Incorporating Errata Corrections based on such documents,
  provided that attribution be made to the OIDF as the source of the
  material, but that such attribution does not indicate an endorsement by the
  OIDF." A purpose-limited licence does not reach every reader of a public
  repository, so the documents are not redistributed here.
- Read by: #88 (the research comparing the B.4 and B.4a tracks, and the
  B.4a track it files)

| Document | URL | sha256 |
|---|---|---|$rows
PROV

say "${#documents[@]} documents in $cache"
say "done"
