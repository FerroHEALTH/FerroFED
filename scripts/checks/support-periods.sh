#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The support-period guard (#1062), for Regulation (EU) 2024/2847 Art 13(8)
# and (19) and Annex II point 7. How it checks is our own design: no
# specification governs it.
#
# SECURITY.md is the record of every release's support period: its support
# table, the rows above the marker line, gets one row per release from
# scripts/release/changelog.sh --assemble. The book's CRA information to the
# user repeats that table under its point 7 heading. This guard fails when
# the two hold different rows, naming each row one of them lacks, so the book
# page never states an end date SECURITY.md does not.
#
# A row is a table line whose first cell is a version, vX.Y.Z; its three
# cells are compared trimmed, so the two tables may differ in their header
# and separator lines and in cell padding.
#
# Usage:
#   scripts/checks/support-periods.sh
#   scripts/checks/support-periods.sh --self-test
# Needs awk, sort and comm. Exit 1 naming each difference; 2 on a usage
# error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly RECORD=SECURITY.md
readonly PAGE=website/book/src/evaluate/cra-user-information.md
readonly MARKER='<!-- support periods: rows above this line -->'
readonly RECORD_HEADING='## Supported versions'
readonly PAGE_HEADING='## 7. Technical security support and the support period'

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

# rows FILE HEADING [MARKER]: the version rows of the table under HEADING in
# FILE, up to the next `## ` heading or MARKER, one per line as
# `version|released|until`, sorted.
rows() {
  awk -v heading="$2" -v marker="${3:-}" '
    $0 == heading { inside = 1; next }
    inside && (/^## / || (marker != "" && $0 == marker)) { inside = 0 }
    inside && /^\|/ {
      n = split($0, c, "|")
      if (n != 5) next
      for (i = 2; i <= 4; i++) gsub(/^[[:space:]]+|[[:space:]]+$/, "", c[i])
      if (c[2] ~ /^v[0-9]+\.[0-9]+\.[0-9]+$/) print c[2] "|" c[3] "|" c[4]
    }
  ' "$1" | sort
}

run() {
  local work failures=0 row count
  for f in "$RECORD" "$PAGE"; do
    [[ -f "$f" ]] || {
      echo "::error::support-periods: $f is missing." >&2
      return 1
    }
  done
  grep -q -F -x -- "$MARKER" "$RECORD" || {
    echo "::error::support-periods: $RECORD has no support table ending in the line '$MARKER'." >&2
    return 1
  }
  work="$(mktemp -d)"
  rows "$RECORD" "$RECORD_HEADING" "$MARKER" > "$work/record"
  rows "$PAGE" "$PAGE_HEADING" > "$work/page"
  if [[ ! -s "$work/record" ]]; then
    echo "::error::support-periods: $RECORD lists no release under \"$RECORD_HEADING\"." >&2
    failures=$((failures + 1))
  fi
  while IFS= read -r row; do
    echo "::error::support-periods: $PAGE lacks the row ${row//|/ | } that $RECORD lists; copy it under \"${PAGE_HEADING#\#\# }\"." >&2
    failures=$((failures + 1))
  done < <(comm -23 "$work/record" "$work/page")
  while IFS= read -r row; do
    echo "::error::support-periods: $PAGE lists the row ${row//|/ | }, which $RECORD does not." >&2
    failures=$((failures + 1))
  done < <(comm -13 "$work/record" "$work/page")
  count="$(grep -c '' "$work/record" || true)"
  rm -rf -- "$work"
  if [[ "$failures" -gt 0 ]]; then
    echo "support-periods: $failures differences." >&2
    return 1
  fi
  echo "support-periods: $PAGE lists the $count support periods $RECORD records."
}

self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  repo="$work/repo"
  mkdir -p "$repo/scripts/checks" "$repo/website/book/src/evaluate"
  cp "$script" "$repo/scripts/checks/support-periods.sh"
  flunk() {
    echo "support-periods: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  expect() {
    local name="$1" want="$2" pattern="${3:-}"
    status=0
    (cd "$repo" && bash scripts/checks/support-periods.sh) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
    [[ -z "$pattern" ]] || grep -Fq -- "$pattern" "$work/out" || flunk "$name did not say \"$pattern\"."
  }
  # The fixture: two releases in each table, padded differently, with a
  # withdrawn-versions table below the marker and a table under another
  # heading of the page, neither of which is read.
  fixture() {
    printf '%s\n' '# Security' '' "$RECORD_HEADING" '' \
      '| Release | Released | Fixes published until |' '| ------- | -------- | --------------- |' \
      '| v0.0.1 | 2026-10-01 | 2031-10-01 |' '| v0.0.2 | 2026-10-02 | 2031-10-02 |' \
      "$MARKER" '' '| v0.0.9 | 2026-10-09 | withdrawn |' '' '## Withdrawn versions' '' \
      '| Version | Withdrawn |' '|---|---|' '| v0.0.8 | 2026-10-09 |' > "$repo/$RECORD"
    printf '%s\n' '# CRA information' '' '## 6. Another point' '' \
      '| Release | Released | Fixes published until |' '|---|---|---|' '| v0.0.7 | 2026-10-07 | 2031-10-07 |' '' \
      "$PAGE_HEADING" '' '| Release | Released | Fixes published until |' '|---|---|---|' \
      '|  v0.0.1  | 2026-10-01 | 2031-10-01 |' '| v0.0.2 | 2026-10-02 |  2031-10-02 |' '' \
      '## 8. Instructions' '' '| v0.0.5 | 2026-10-05 | 2031-10-05 |' > "$repo/$PAGE"
  }
  # edit FILE SED: rewrite one fixture file with a sed program.
  edit() {
    sed -e "$2" "$repo/$1" > "$work/edit" && mv "$work/edit" "$repo/$1"
  }

  fixture
  expect "two tables with the same rows" 0 "lists the 2 support periods"
  edit "$PAGE" '/^| v0.0.2 /d'
  expect "a page that lacks a row" 1 "lacks the row v0.0.2 | 2026-10-02 | 2031-10-02"
  fixture
  edit "$PAGE" 's/2031-10-02/2031-10-03/'
  expect "a page with a different end date" 1 "lists the row v0.0.2 | 2026-10-02 | 2031-10-03"
  fixture
  edit "$PAGE" '/^| v0.0.2 /a\
| v0.0.3 | 2026-10-03 | 2031-10-03 |
'
  expect "a page with a row the record lacks" 1 "lists the row v0.0.3"
  fixture
  edit "$RECORD" "/^| v0.0.2 /a\\
| v0.0.3 | 2026-10-03 | 2031-10-03 |
"
  expect "a release cut that updated only the record" 1 "lacks the row v0.0.3"
  fixture
  edit "$RECORD" "/^$MARKER\$/d"
  expect "a record with no marker" 1 "has no support table ending in the line"
  fixture
  edit "$RECORD" '/^| v0.0.[12] /d'
  edit "$PAGE" '/^|  *v0.0.[12] /d'
  expect "a record with no release" 1 "lists no release"
  rm -f "$repo/$PAGE"
  expect "a missing page" 1 "$PAGE is missing"
  echo "support-periods: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
"")
  cd "$(dirname "$0")/../.."
  run
  ;;
*) usage ;;
esac
