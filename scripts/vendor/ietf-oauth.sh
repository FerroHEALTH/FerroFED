#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ietf-oauth.sh
#
# Vendors the IETF RFCs the onward authentication tracks of Annex B rest on
# into docs/specs/ietf-oauth/, each as the plain-text RFC the RFC Editor
# publishes: OAuth 2.0 (RFC 6749), JWT (RFC 7519), the assertion framework
# and its JWT profile (RFC 7521, RFC 7523), token introspection (RFC 7662),
# authorization server metadata (RFC 8414), Pushed Authorization Requests
# (RFC 9126), Rich Authorization Requests (RFC 9396), DPoP (RFC 9449) and
# mutual-TLS client authentication with certificate-bound tokens (RFC 8705).
#
# The GF-Authentication pages of the Netherlands Generic Functions IG cite
# RFC 6749, 7523, 7662 and 9449; Nuts RFC021 builds on RFC 7521, 7519 and
# 8414; the BgZ/eOverdracht track of Annex B §B.4a names RFC 9126 (through
# FAPI 2.0) and RFC 9396. The crate crates/oauth-server-metadata (#551) reads
# RFC 8414, and the Nuts grant of crates/nl-generic-functions feature
# `nuts-auth` (#88) and the FAPI 2.0 grant of app/ferrofed-engine (#497) hold
# their authorization server metadata to it. The Nuts grant reads RFC 9449,
# and the onward grants of app/ferrofed-engine read RFC 8705 (#492).
#
# An RFC never changes once published, so each "IETF RFC <n>" row of
# docs/VERSIONS.md pins its URL and the sha256 of its bytes; the script fails
# when a download differs.
#
# Redistribution: every RFC is subject to BCP 78 and the IETF Trust Legal
# Provisions, whose §3.c.i grants every person the right to copy, publish,
# display and distribute IETF Documents in full and without modification. The
# script fails if an RFC stops carrying that notice.
#
# Usage:
#   scripts/vendor/ietf-oauth.sh
#
# Requires: curl, shasum.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum

dest="docs/specs/ietf-oauth"
numbers=(6749 7519 7521 7523 7662 8414 8705 9126 9396 9449)
notice="This document is subject to BCP 78 and the IETF Trust's Legal"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

pins=""
for number in "${numbers[@]}"; do
  item="IETF RFC $number"
  say "fetching RFC $number"
  corpus_fetch_pinned "$item" "$tmp/rfc$number.txt"
  grep -qF "$notice" "$tmp/rfc$number.txt" \
    || die "RFC $number does not carry the BCP 78 notice"
  pin="$(corpus_pin_cell "$item")"
  pins="$pins
- RFC $number: <$(corpus_pin_url "$pin")>, sha256 \`$(corpus_pin_sha256 "$pin")\`"
done

rm -rf "$dest"
mkdir -p "$dest"
for number in "${numbers[@]}"; do
  cp "$tmp/rfc$number.txt" "$dest/rfc$number.txt"
done

rows=""
while IFS= read -r file; do
  path="${file#"$dest"/}"
  rows="$rows
| \`$path\` | \`$(corpus_sha256 "$file")\` |"
done < <(find "$dest" -type f ! -name PROVENANCE.md | LC_ALL=C sort)

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the IETF OAuth RFCs

Vendored verbatim by \`scripts/vendor/ietf-oauth.sh\`. Never edit a file here:
change the pins in docs/VERSIONS.md and re-run the script.

- Source: the RFC Editor, <https://www.rfc-editor.org/>, the plain-text
  publication of each RFC
- Pins, one row of docs/VERSIONS.md per RFC, by URL and sha256:$pins
- Fetched: $fetched
- Upstream licence: each RFC carries the notice "$notice
  Provisions Relating to IETF Documents (https://trustee.ietf.org/license-info)
  in effect on the date of publication of this document". The IETF Trust
  Legal Provisions 5.0, §3.c, grant every person "a non-exclusive,
  royalty-free, worldwide right and license under all copyrights and rights
  of authors: i. to copy, publish, display and distribute IETF Contributions
  and IETF Documents in full and without modification". The files are those
  documents, in full and unmodified.
- Layout: \`rfc<number>.txt\`, the RFC Editor's file name
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #551 (the authorization server metadata of RFC 8414 in
  \`crates/oauth-server-metadata\`, which the Nuts grant of #88 and the
  FAPI 2.0 grant of #497 hold their metadata to), #88 (the access token
  request of \`crates/nl-generic-functions\` feature \`nuts-auth\`: the DPoP
  proof of RFC 9449; the research comparing the B.4 and B.4a tracks),
  #492 (the mutual-TLS client authentication and certificate-bound tokens
  of RFC 8705 on the onward grants)

## What is taken

The RFCs the GF-Authentication pages of the Netherlands Generic Functions IG
cite (6749, 7523, 7662, 9449), the ones Nuts RFC021 builds its grant on
(7519, 7521, 8414), and the two the BgZ/eOverdracht track of Annex B §B.4a
adds (9126, 9396), and RFC 8705, which the onward grants authenticate and
bind their tokens with when a deployment uses mutual TLS. The other RFCs
the OAuth family references are cited, not taken.

| File | sha256 |
|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
