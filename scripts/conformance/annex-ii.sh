#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/annex-ii.sh: the book page of FerroFED's Annex II
# checklist (#941, #1667), for Regulation (EU) 2025/327.
#
#   scripts/conformance/annex-ii.sh --page-block  print the page block
#   scripts/conformance/annex-ii.sh --write       write it in place
#
# conformance/annex-ii.tsv is the source. The block between the
# annex-ii:begin and annex-ii:end markers of
# website/book/src/evaluate/annex-ii.md holds one table per section of
# Annex II, what is missing for each partial or planned point, and the
# notes on the met and not-applicable points. scripts/checks/annex-ii.sh
# holds the page to a fresh rendering. No specification governs this: our
# own design.
set -euo pipefail
export LC_ALL=C

# The awk functions both renderers share (#1675).
# shellcheck source=scripts/checks/lib/evidence.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$(dirname "$0")/../checks/lib/evidence.sh"

readonly CHECKLIST=conformance/annex-ii.tsv
readonly PAGE=website/book/src/evaluate/annex-ii.md
readonly BOOK=website/book/src/
readonly REPO_URL=https://github.com/FerroHEALTH/FerroFED
readonly BEGIN='<!-- annex-ii:begin -->'
readonly END='<!-- annex-ii:end -->'

die() {
  echo "annex-ii: $*" >&2
  exit 2
}

usage() {
  sed -n '/^#   scripts/p' "$0" >&2
  exit 2
}

# page_block: the rendered block, markers included.
page_block() {
  [[ -f "$CHECKLIST" ]] || die "$CHECKLIST is missing"
  printf '%s\n' "$BEGIN"
  printf '<!-- Rendered from %s by scripts/conformance/annex-ii.sh --write. Do not edit between the markers. -->\n' "$CHECKLIST"
  awk -F'\t' -v book="$BOOK" -v repo="$REPO_URL" "$(links_awk)"'
    function table(section, heading,   i) {
      print ""
      print "## " heading
      print ""
      print "| Point | Requirement | Evidence | Status | Issue |"
      print "|---|---|---|---|---|"
      for (i = 1; i <= n; i++)
        if (substr(point[i], 1, 1) == section)
          print "| " point[i] " | \"" req[i] "\" | " evidence(ev[i]) " | " st[i] " | " issues(iss[i]) " |"
    }
    /^#/ || /^$/ { next }
    $1 == "point" { next }
    {
      n++
      point[n] = $1; req[n] = $2; st[n] = $3; ev[n] = $4; iss[n] = $5; note[n] = $6
    }
    END {
      table("1", "1. General requirements")
      table("2", "2. Requirements for interoperability")
      table("3", "3. Requirements for security and logging")
      print ""
      print "## What is missing"
      print ""
      for (i = 1; i <= n; i++)
        if (st[i] == "partial" || st[i] == "planned") print "- **" point[i] ":** " note[i]
      print ""
      print "## Notes on the met and not-applicable points"
      print ""
      for (i = 1; i <= n; i++)
        if ((st[i] == "met" || st[i] == "not-applicable") && note[i] != "-") print "- **" point[i] ":** " note[i]
      print ""
    }
  ' "$CHECKLIST"
  printf '%s\n' "$END"
}

# write: replace the block of the page with a fresh rendering.
write() {
  local block tmp
  [[ -f "$PAGE" ]] || die "$PAGE is missing"
  if ! grep -Fxq "$BEGIN" "$PAGE" || ! grep -Fxq "$END" "$PAGE"; then
    die "$PAGE has no $BEGIN and $END markers"
  fi
  block="$(mktemp)"
  tmp="$(mktemp)"
  page_block > "$block"
  awk -v begin="$BEGIN" -v end="$END" -v block="$block" '
    $0 == begin { while ((getline l < block) > 0) print l; skip = 1; next }
    $0 == end { skip = 0; next }
    !skip { print }
  ' "$PAGE" > "$tmp"
  mv "$tmp" "$PAGE"
  rm -f -- "$block"
}

cd "$(dirname "$0")/../.."
case "${1:-}" in
--page-block)
  [[ $# -eq 1 ]] || usage
  page_block
  ;;
--write)
  [[ $# -eq 1 ]] || usage
  write
  ;;
*) usage ;;
esac
