#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/myhealth-eu.sh: the renderings of FerroFED's
# self-assessment against the MyHealth@EU Requirements Catalogue (#1423).
#
#   scripts/conformance/myhealth-eu.sh --badge [NAME]  print a badge file
#   scripts/conformance/myhealth-eu.sh --readme-block  print the README block
#   scripts/conformance/myhealth-eu.sh --page-block    print the book page block
#   scripts/conformance/myhealth-eu.sh --write         write them all in place
#
# conformance/myhealth-eu.tsv is the source. The badge is a shields.io
# endpoint file (https://shields.io/badges/endpoint-badge) counting the rows
# met among the rows that bind the gateway, coloured by that count (green
# when all are met, yellow when some are, red when none are; owner decision,
# 2026-11-25), with the EU emblem on EU blue. No badge repeats that its count is self-assessed or self-tested:
# the README caption under the badge heading says it once (owner decision,
# 2026-11-25). Beside it, on the MyHealth@EU
# CI line (#1952), one badge per NAME of BADGES: the NCPeH API
# DocumentReference search parameters the gateway answers, from conformance/myhealth-eu-search.tsv, and the
# documents valid against MyHealth@EU Core, Laboratory Report and Hospital
# Discharge Report, from conformance/myhealth-eu-validation.tsv, which the
# validator lane holds to its run (scripts/checks/fhir-validator.sh
# --myhealth-eu). Each label names its guide at the version its pinned
# package manifest names. The README block between the myhealth-eu:begin and
# myhealth-eu:end markers links the badges to the book page, and the page
# block between the same markers in website/book/src/evaluate/myhealth-eu.md
# is the table of every row. scripts/checks/myhealth-eu.sh holds all three to
# a fresh rendering. No specification governs this: our own design.
set -euo pipefail
export LC_ALL=C
# shellcheck source=scripts/checks/badge-logos/logo.sh
source "$(dirname "${BASH_SOURCE[0]}")/../checks/badge-logos/logo.sh"

readonly MAPPING=conformance/myhealth-eu.tsv
readonly BADGE=conformance/myhealth-eu-badge.json
readonly README=README.md
readonly PAGE=website/book/src/evaluate/myhealth-eu.md
readonly CATALOGUE='v10.0.0'
readonly BOOK_PAGE='https://ferrofed.eu/docs/evaluate/myhealth-eu.html'
readonly RAW='https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2F'
readonly ENDPOINT="https://img.shields.io/endpoint?url=${RAW}myhealth-eu-badge.json"
readonly SEARCH=conformance/myhealth-eu-search.tsv
readonly VALIDATION=conformance/myhealth-eu-validation.tsv

# One "name<TAB>package<TAB>title<TAB>suffix" record per CI-line badge: the
# pinned package whose manifest names the version, the guide's title on the
# badge and what the version carries after it.
readonly BADGES="\
search	docs/specs/ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz	MyHealth@EU NCP API	 DocumentReference search
core	docs/specs/ehdsi-core/myhealth.eu.fhir.core-1.0.1.tgz	MyHealth@EU Core	 (CI)
laboratory	docs/specs/ehdsi-laboratory/myhealth.eu.fhir.laboratory-9.2.0.tgz	MyHealth@EU Laboratory	 (CI)
discharge	docs/specs/ehdsi-hdr/myhealth.eu.fhir.hdr-9.2.0.tgz	MyHealth@EU HDR	 (CI)"

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

# data FILE: the data rows of FILE, without comments and its header.
data() {
  [[ -f "$1" ]] || die "$1 is missing"
  awk -F'\t' '/^#/ || /^$/ { next } NR == 1 || $1 == "guide" || $1 == "parameter" { next } { print }' "$1"
}

# version PACKAGE: the version the package manifest of PACKAGE names.
version() {
  tar -xzOf "$1" package/package.json | jq -r '.version // empty'
}

# ci_badge NAME: the badge file of the CI-line badge NAME.
ci_badge() {
  local name="$1" record package title suffix label message color
  record="$(awk -F'\t' -v name="$name" '$1 == name' <<< "$BADGES")"
  [[ -n "$record" ]] || die "no badge is named $name"
  IFS=$'\t' read -r _ package title suffix <<< "$record"
  label="$title $(version "$package")$suffix"
  if [[ "$name" == search ]]; then
    message="$(data "$SEARCH" | awk -F'\t' '{ n++ } $3 == "answered" { a++ } END { printf "%d/%d parameters answered", a, n }')"
  else
    message="$(data "$VALIDATION" | awk -F'\t' -v guide="$name" '$1 == guide { n++; if ($3 == "valid") v++ } END { printf "%d/%d valid", v, n }')"
  fi
  [[ "$message" =~ ^([0-9]+)/([0-9]+) ]] || die "the $name badge counts nothing: $message"
  color="$(badge_count_color "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}")"
  jq -cn --arg label "$label" --arg message "$message" --arg color "$color" \
    --argjson logo "$(badge_logo_fields eu)" \
    '{schemaVersion: 1, label: $label, message: $message, color: $color} + $logo'
}

badge() {
  case "${1:-self-assessed}" in
  self-assessed)
    jq -cn --arg message "catalogue $CATALOGUE · $(count met)/$(count) met" \
      --arg color "$(badge_count_color "$(count met)" "$(count)")" --argjson logo "$(badge_logo_fields eu)" \
      '{schemaVersion: 1, label: "MyHealth@EU", message: $message, color: $color} + $logo'
    ;;
  *) ci_badge "$1" ;;
  esac
}

# badge_file NAME: where the badge NAME is written.
badge_file() {
  case "$1" in
  self-assessed) echo "$BADGE" ;;
  *) echo "conformance/myhealth-eu/$1.json" ;;
  esac
}

# badge_names: every badge, the self-assessment first.
badge_names() {
  echo self-assessed
  awk -F'\t' '{ print $1 }' <<< "$BADGES"
}

readme_block() {
  local name
  echo '<!-- myhealth-eu:begin -->'
  echo "[![MyHealth@EU self-assessed](${ENDPOINT})](${BOOK_PAGE})"
  awk -F'\t' '{ print $1 }' <<< "$BADGES" | while read -r name; do
    echo "[![MyHealth@EU ${name}, self-tested](https://img.shields.io/endpoint?url=${RAW}myhealth-eu%2F${name}.json)](${BOOK_PAGE})"
  done
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
  local block name
  badge > "$BADGE"
  # A tree without the pinned packages, such as a guard's fixture, writes the
  # self-assessment alone; scripts/checks/myhealth-eu-ci.sh holds the rest.
  if [[ -f "$SEARCH" && -f "$VALIDATION" ]]; then
    mkdir -p conformance/myhealth-eu
    while IFS=$'\t' read -r name _; do
      badge "$name" > "$(badge_file "$name")"
    done <<< "$BADGES"
  fi
  block="$(mktemp)"
  readme_block > "$block"
  replace_block "$README" "$block"
  page_block > "$block"
  replace_block "$PAGE" "$block"
  rm -f -- "$block"
  echo "myhealth-eu: wrote the badges, the $README block and the $PAGE block."
}

cd "$(dirname "$0")/../.."
case "${1:-}" in
--badge) badge "${2:-self-assessed}" ;;
--badge-file) badge_file "${2:?a badge name}" ;;
--badge-names) badge_names ;;
--readme-block) readme_block ;;
--page-block) page_block ;;
--write) write ;;
*) die "usage: $0 --badge [NAME] | --badge-file NAME | --badge-names | --readme-block | --page-block | --write" ;;
esac
