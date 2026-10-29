#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/myhealth-eu.sh: the renderings of FerroFED's
# self-assessment against the MyHealth@EU Requirements Catalogue (#1423).
#
#   scripts/conformance/myhealth-eu.sh --badge         print the badge file
#   scripts/conformance/myhealth-eu.sh --readme-block  print the README block
#   scripts/conformance/myhealth-eu.sh --page-block    print the book page block
#   scripts/conformance/myhealth-eu.sh --write         write all three in place
#
# conformance/myhealth-eu.tsv is the source. The badge is a shields.io
# endpoint file (https://shields.io/badges/endpoint-badge) counting the rows
# met among the rows that bind the gateway, always blue and always marked
# self-assessed. The README block between the myhealth-eu:begin and
# myhealth-eu:end markers links the badge to the book page, and the page
# block between the same markers in website/book/src/evaluate/myhealth-eu.md
# is the table of every row. scripts/checks/myhealth-eu.sh holds all three to
# a fresh rendering. No specification governs this: our own design.
set -euo pipefail
export LC_ALL=C

readonly MAPPING=conformance/myhealth-eu.tsv
readonly BADGE=conformance/myhealth-eu-badge.json
readonly README=README.md
readonly PAGE=website/book/src/evaluate/myhealth-eu.md
readonly CATALOGUE='v10.0.0'
readonly BOOK_PAGE='https://ferrofed.eu/docs/evaluate/myhealth-eu.html'
readonly ENDPOINT='https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fmyhealth-eu-badge.json'

die() {
  echo "myhealth-eu: $*" >&2
  exit 2
}

# rows: the data rows of the mapping, without comments and the header.
rows() {
  [[ -f "$MAPPING" ]] || die "$MAPPING is missing"
  awk -F'\t' '/^#/ || /^$/ { next } $1 == "id" { next } { print }' "$MAPPING"
}

# count [STATUS]: the number of rows binding the gateway, or with STATUS.
count() {
  rows | awk -F'\t' -v status="${1:-}" '
    status == "" && $3 != "not-applicable" { n++ }
    status != "" && $3 == status { n++ }
    END { print n + 0 }
  '
}

badge() {
  printf '{"schemaVersion":1,"label":"MyHealth@EU","message":"catalogue %s · %s/%s met, self-assessed","color":"blue"}\n' \
    "$CATALOGUE" "$(count met)" "$(count)"
}

readme_block() {
  echo '<!-- myhealth-eu:begin -->'
  echo "[![MyHealth@EU self-assessment](${ENDPOINT})](${BOOK_PAGE})"
  echo '<!-- myhealth-eu:end -->'
}

page_block() {
  local total binding
  total="$(rows | wc -l | tr -d ' ')"
  binding="$(count)"
  echo '<!-- myhealth-eu:begin -->'
  echo '<!-- Rendered from conformance/myhealth-eu.tsv by scripts/conformance/myhealth-eu.sh --write. Do not edit between the markers. -->'
  echo
  printf 'Of the %s rows, %s bind the gateway: %s met, %s partly met, %s waiting on the Art 15(1) act, %s not met and %s not assessed. The other %s do not apply to the gateway.\n' \
    "$total" "$binding" "$(count met)" "$(count partly-met)" "$(count waits-art-15-1)" \
    "$(count not-met)" "$(count not-assessed)" "$(count not-applicable)"
  echo
  echo '| Id | Section | Status | FerroFED'"'"'s conclusion | Evidence | Issue |'
  echo '|---|---|---|---|---|---|'
  rows | awk -F'\t' '
    {
      evidence = ""
      n = split($4, items, "; ")
      for (k = 1; k <= n; k++) evidence = evidence (k > 1 ? "<br>" : "") "`" items[k] "`"
      printf "| %s | §%s | %s | %s | %s | %s |\n", $1, $2, $3, $6, evidence, $5
    }
  '
  echo
  echo '<!-- myhealth-eu:end -->'
}

# replace_block FILE RENDERED: put RENDERED in place of the block of FILE.
replace_block() {
  local file="$1" rendered="$2" tmp
  if ! grep -qx '<!-- myhealth-eu:begin -->' "$file" || ! grep -qx '<!-- myhealth-eu:end -->' "$file"; then
    die "$file has no myhealth-eu:begin and myhealth-eu:end markers"
  fi
  tmp="$(mktemp)"
  awk -v block="$rendered" '
    $0 == "<!-- myhealth-eu:begin -->" { while ((getline line < block) > 0) print line; skipping = 1; next }
    $0 == "<!-- myhealth-eu:end -->" { skipping = 0; next }
    !skipping { print }
  ' "$file" > "$tmp"
  cat "$tmp" > "$file"
  rm -f -- "$tmp"
}

write() {
  local block
  badge > "$BADGE"
  block="$(mktemp)"
  readme_block > "$block"
  replace_block "$README" "$block"
  page_block > "$block"
  replace_block "$PAGE" "$block"
  rm -f -- "$block"
  echo "myhealth-eu: wrote $BADGE, the $README block and the $PAGE block."
}

cd "$(dirname "$0")/../.."
case "${1:-}" in
--badge) badge ;;
--readme-block) readme_block ;;
--page-block) page_block ;;
--write) write ;;
*) die "usage: $0 --badge | --readme-block | --page-block | --write" ;;
esac
