#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The Annex II checklist guard (#941), for the essential requirements of
# Annex II of Regulation (EU) 2025/327 (Art 30(1)(a), Art 37(2)). How it
# checks is our own design: no specification governs it.
#
# conformance/annex-ii.tsv has one row for every point and sub-point of
# Annex II. The guard holds each row to:
#
#   the text      its requirement is a phrase of that point's own text in
#                 the vendored Regulation, between the point's number and
#                 the next one;
#   the tree      each evidence item is a file of the tree, and its anchor,
#                 when it has one, is a heading of a Markdown file or a test
#                 function of a Rust file;
#   the status    met, partial, planned or not-applicable; a partial or
#                 planned row names its issues and what is missing, a
#                 not-applicable row says why;
#   the book      the page lists every point with its status, and the
#                 technical documentation index does not mark Annex III,
#                 point 4, documented while a point is partial or planned.
#
# Usage:
#   scripts/checks/annex-ii.sh
#   scripts/checks/annex-ii.sh --self-test
# Needs awk, grep, sed and tr. Exit 1 naming each refusal; 2 on a usage
# error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly CHECKLIST=conformance/annex-ii.tsv
readonly PAGE=website/book/src/evaluate/annex-ii.md
readonly SUMMARY=website/book/src/SUMMARY.md
readonly INDEX=conformance/technical-documentation.tsv
readonly REGULATION=docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml
readonly POINTS='1.1 1.2 1.3 1.4 2.1 2.2 2.3 2.4 2.5 2.6 3.1 3.2 3.2(a) 3.2(b) 3.2(c) 3.2(d) 3.2(e) 3.3 3.4'
readonly HEADER=$'point\trequirement\tstatus\tevidence\tissue\tnote'

failures=0

fail() {
  echo "::error::annex-ii: $*" >&2
  failures=$((failures + 1))
}

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

# segments OUT: one line per point of Annex II, the point, a tab and the
# point's own text, from the vendored Regulation with its markup and
# no-break spaces turned into single spaces. A point's text runs from its
# number to the next point's; the last runs to Annex III.
segments() {
  local nbsp
  nbsp="$(printf '\302\240')"
  tr '\n\r\t' '   ' < "$REGULATION" |
    sed -e 's/<[^>]*>/ /g' -e "s/$nbsp/ /g" -e 's/&amp;/\&/g' -e 's/  */ /g' |
    awk -v points="$POINTS" '
      {
        start = index($0, "ANNEX II Essential requirements")
        if (start == 0) exit
        text = substr($0, start)
        stop = index(text, " ANNEX III")
        if (stop > 0) text = substr(text, 1, stop - 1)
        n = split(points, p, " ")
        pos = 1
        for (k = 1; k <= n; k++) {
          marker = p[k] "."
          if (p[k] ~ /\(/) marker = substr(p[k], length(p[k]) - 2)
          i = index(substr(text, pos), marker)
          if (i == 0) { at[k] = 0; continue }
          at[k] = pos + i - 1
          pos = at[k] + length(marker)
        }
        for (k = 1; k <= n; k++) {
          if (at[k] == 0) continue
          end = length(text) + 1
          for (j = k + 1; j <= n; j++) if (at[j] > 0) { end = at[j]; break }
          printf "%s\t%s\n", p[k], substr(text, at[k], end - at[k])
        }
      }
    ' > "$1"
}

# has_heading FILE TEXT: FILE has a Markdown heading reading TEXT, outside a
# fenced code block.
has_heading() {
  awk -v want="$2" '
    /^```/ { fence = !fence; next }
    !fence && /^#+ / { h = $0; sub(/^#+ /, "", h); if (h == want) found = 1 }
    END { exit !found }
  ' "$1"
}

# has_test FILE NAME: FILE defines the function NAME under a test attribute,
# among the attributes and comments directly above it.
has_test() {
  awk -v want="$2" '
    { line[NR] = $0 }
    $0 ~ ("fn " want "\\(") {
      for (k = NR - 1; k > 0 && line[k] ~ /^[[:space:]]*(#\[|\/\/)/; k--)
        if (line[k] ~ /^[[:space:]]*#\[.*test(\]|\()/) found = 1
    }
    END { exit !found }
  ' "$1"
}

# check_evidence LINE EVIDENCE: every item of EVIDENCE, split on "; ".
check_evidence() {
  local line="$1" rest="$2" item path anchor
  [[ "$rest" != - && -n "$rest" ]] || {
    fail "$CHECKLIST line $line names no evidence."
    return
  }
  while [[ -n "$rest" ]]; do
    item="${rest%%; *}"
    if [[ "$item" == "$rest" ]]; then rest=""; else rest="${rest#*; }"; fi
    path="${item%%::*}"
    anchor=""
    [[ "$path" == "$item" ]] || anchor="${item#*::}"
    case "$path" in
    /* | *..* | '')
      fail "$CHECKLIST line $line: \"$path\" is not a path inside the tree."
      continue
      ;;
    esac
    if [[ ! -f "$path" ]]; then
      fail "$CHECKLIST line $line: $path does not exist."
      continue
    fi
    [[ -n "$anchor" ]] || continue
    case "$path" in
    *.md) has_heading "$path" "$anchor" ||
      fail "$CHECKLIST line $line: $path has no heading \"$anchor\"." ;;
    *.rs) has_test "$path" "$anchor" ||
      fail "$CHECKLIST line $line: $path has no test \"$anchor\"." ;;
    *) fail "$CHECKLIST line $line: $path takes no anchor; only a Markdown heading or a Rust test does." ;;
    esac
  done
}

run() {
  local text row line=0 seen point requirement status evidence issue note
  local own unmet=""
  for f in "$CHECKLIST" "$PAGE" "$SUMMARY" "$INDEX" "$REGULATION"; do
    [[ -f "$f" ]] || {
      fail "$f is missing."
      finish
      return
    }
  done
  text="$(mktemp)"
  segments "$text"
  seen=" "
  while IFS= read -r row; do
    line=$((line + 1))
    case "$row" in '#'* | '') continue ;; esac
    if [[ "$(printf '%s\n' "$row" | awk -F'\t' '{ print NF }')" != 6 ]]; then
      fail "$CHECKLIST line $line has not six tab-separated columns."
      continue
    fi
    IFS=$'\t' read -r point requirement status evidence issue note <<< "$row"
    if [[ "$point" == point ]]; then
      [[ "$row" == "$HEADER" ]] ||
        fail "$CHECKLIST header is not point, requirement, status, evidence, issue, note."
      continue
    fi
    case " $POINTS " in
    *" $point "*) ;;
    *)
      fail "$CHECKLIST line $line: $point is not a point of Annex II."
      continue
      ;;
    esac
    case "$seen" in
    *" $point "*) fail "$CHECKLIST line $line: $point has a second row." ;;
    *) seen="$seen$point " ;;
    esac
    own="$(awk -F'\t' -v p="$point" '$1 == p { print $2 }' "$text")"
    if [[ -z "$own" ]]; then
      fail "$CHECKLIST line $line: $REGULATION has no point $point in Annex II."
    elif [[ "$own" != *"$requirement"* ]]; then
      fail "$CHECKLIST line $line: \"$requirement\" is not a phrase of point $point of $REGULATION."
    fi
    check_evidence "$line" "$evidence"
    case "$status" in
    met | not-applicable)
      [[ "$issue" == - ]] || fail "$CHECKLIST line $line: a $status row names no issue."
      [[ "$status" == met || "$note" != - ]] ||
        fail "$CHECKLIST line $line: a not-applicable row says why in its note."
      ;;
    partial | planned)
      unmet="$unmet $point"
      [[ "$issue" =~ ^#[0-9]+( #[0-9]+)*$ ]] ||
        fail "$CHECKLIST line $line: a $status row names its issues as #N, separated by one space."
      [[ "$note" != - ]] || fail "$CHECKLIST line $line: a $status row says what is missing in its note."
      ;;
    *) fail "$CHECKLIST line $line: status \"$status\" is not met, partial, planned or not-applicable." ;;
    esac
    [[ -z "$status" ]] || grep -Eq "^\| $(printf '%s' "$point" | sed 's/[.()]/\\&/g') \|.*\| $status \|" "$PAGE" ||
      fail "$PAGE lists no row for $point with the status $status."
  done < "$CHECKLIST"
  rm -f -- "$text"
  for point in $POINTS; do
    case "$seen" in
    *" $point "*) ;;
    *) fail "$CHECKLIST has no row for $point." ;;
    esac
  done
  grep -Fq '(evaluate/annex-ii.md)' "$SUMMARY" || fail "$SUMMARY does not list $PAGE."
  awk -F'\t' -v page="$PAGE" '$1 == "4" && $3 == page { found = 1 } END { exit !found }' "$INDEX" ||
    fail "$INDEX has no row for Annex III, point 4, that names $PAGE."
  if [[ -n "$unmet" ]] && awk -F'\t' '$1 == "4" && $5 == "documented" { found = 1 } END { exit !found }' "$INDEX"; then
    fail "$INDEX marks Annex III, point 4, documented while Annex II point(s)$unmet are not met."
  fi
  finish
}

finish() {
  if [[ "$failures" -gt 0 ]]; then
    echo "annex-ii: $failures refusals." >&2
    exit 1
  fi
  echo "annex-ii: every Annex II point has its row, its phrase and its evidence."
}

self_test() {
  local script status point
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  repo="$work/repo"
  mkdir -p "$repo/scripts/checks" "$repo/conformance" "$repo/docs/specs/eu-ehds" \
    "$repo/website/book/src/evaluate" "$repo/tests"
  cp "$script" "$repo/scripts/checks/annex-ii.sh"
  flunk() {
    echo "annex-ii: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  expect() {
    local name="$1" want="$2" pattern="${3:-}"
    status=0
    (cd "$repo" && bash scripts/checks/annex-ii.sh) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
    [[ -z "$pattern" ]] || grep -Fq -- "$pattern" "$work/out" || flunk "$name did not say \"$pattern\"."
  }
  # The fixture: a Regulation whose Annex II gives each point the text
  # "rule <point>", split by markup and a no-break space, one row per
  # point, the page, the summary, the index, a heading and a test.
  fixture() {
    {
      printf '<p>rule 1.1 before the annex</p>\n<p>ANNEX II</p><p>Essential requirements</p>\n'
      for point in $POINTS; do
        case "$point" in
        *'('*) printf '<p>%s</p><p>rule\302\240<span>%s</span></p>\n' "${point#3.2}" "$point" ;;
        *) printf '<p>%s.</p><p>rule\302\240<span>%s</span></p>\n' "$point" "$point" ;;
        esac
      done
      printf '<p>ANNEX III</p><p>rule 3.4 after the annex</p>\n'
    } > "$repo/$REGULATION"
    {
      printf '# a comment\n%s\n' "$HEADER"
      for point in $POINTS; do
        printf '%s\trule %s\tmet\t%s::The evidence; tests/it.rs::a_test\t-\t-\n' "$point" "$point" "$PAGE"
      done
    } > "$repo/$CHECKLIST"
    {
      printf '# Annex II\n\n## The evidence\n\n| Point | Requirement | Evidence | Status | Issue |\n|---|---|---|---|---|\n'
      for point in $POINTS; do printf '| %s | rule | here | met | - |\n' "$point"; done
    } > "$repo/$PAGE"
    printf -- '- [Annex II](evaluate/annex-ii.md)\n' > "$repo/$SUMMARY"
    printf 'item\telement\ttarget\tsection\tstatus\tissue\n4\tresults\t%s\t-\tdocumented\t-\n' \
      "$PAGE" > "$repo/$INDEX"
    printf '#[test]\nfn a_test() {}\n\nfn not_a_test() {}\n' > "$repo/tests/it.rs"
  }
  # row POINT FIELD VALUE: set one field of POINT's row; a FIELD of 0 drops it.
  row() {
    awk -F'\t' -v OFS='\t' -v point="$1" -v field="$2" -v value="${3:-}" '
      $1 == point && field == 0 { next }
      $1 == point { $field = value }
      { print }
    ' "$repo/$CHECKLIST" > "$work/checklist" && mv "$work/checklist" "$repo/$CHECKLIST"
  }
  # page POINT STATUS: set the status cell of POINT's page row.
  page() {
    awk -v point="$1" -v status="$2" '
      index($0, "| " point " |") == 1 { $0 = "| " point " | rule | here | " status " | #1 |" }
      { print }
    ' "$repo/$PAGE" > "$work/page" && mv "$work/page" "$repo/$PAGE"
  }

  fixture
  expect "a complete checklist" 0 "every Annex II point has its row"

  row '3.2(c)' 0
  expect "a point with no row" 1 "has no row for 3.2(c)"
  fixture
  grep -F '3.3' "$repo/$CHECKLIST" >> "$repo/$CHECKLIST"
  expect "a point with two rows" 1 "3.3 has a second row"
  fixture
  row 1.2 1 '1.9'
  expect "an unknown point" 1 "1.9 is not a point of Annex II"
  fixture
  row 2.1 2 'rule 2.2'
  expect "a phrase of another point" 1 "is not a phrase of point 2.1"
  fixture
  row 3.4 2 'rule 3.4 after the annex'
  expect "a phrase outside the annex" 1 "is not a phrase of point 3.4"
  fixture
  row '3.2(e)' 2 'rule 3.2(d)'
  expect "a sub-point's phrase from its neighbour" 1 "is not a phrase of point 3.2(e)"
  fixture
  row 1.3 4 'missing.md'
  expect "evidence that does not exist" 1 "missing.md does not exist"
  fixture
  row 1.3 4 '../outside.md'
  expect "evidence outside the tree" 1 "is not a path inside the tree"
  fixture
  row 1.4 4 "$PAGE::No such heading"
  expect "a heading that does not exist" 1 "has no heading \"No such heading\""
  fixture
  row 1.4 4 'tests/it.rs::not_a_test'
  expect "a function that is no test" 1 "has no test \"not_a_test\""
  fixture
  row 1.4 4 "$INDEX::anything"
  expect "an anchor on a file that takes none" 1 "takes no anchor"
  fixture
  row 2.4 4 -
  expect "a row with no evidence" 1 "names no evidence"
  fixture
  row 2.5 3 done
  expect "an unknown status" 1 "status \"done\""
  fixture
  row 2.6 3 not-applicable
  page 2.6 not-applicable
  expect "a not-applicable row with no reason" 1 "says why in its note"
  row 2.6 6 'no such function'
  expect "a not-applicable row with its reason" 0
  fixture
  row 3.1 3 partial
  page 3.1 partial
  expect "a partial row with no issue" 1 "names its issues as #N"
  row 3.1 5 '#12,#13'
  expect "issues not separated by one space" 1 "names its issues as #N"
  row 3.1 5 '#12 #13'
  expect "a partial row with no note" 1 "says what is missing"
  row 3.1 6 'what is missing'
  expect "a partial row while point 4 is documented" 1 "documented while Annex II point(s) 3.1"
  printf 'item\telement\ttarget\tsection\tstatus\tissue\n4\tresults\t%s\t-\tpartial\t#1\n' \
    "$PAGE" > "$repo/$INDEX"
  expect "a partial row with point 4 partial" 0
  fixture
  row 3.3 3 planned
  expect "a status the page does not show" 1 "lists no row for 3.3 with the status planned"
  fixture
  printf '\n' > "$repo/$SUMMARY"
  expect "a page the summary does not list" 1 "does not list"
  fixture
  printf 'item\telement\ttarget\tsection\tstatus\tissue\n4\tresults\tother.md\t-\tdocumented\t-\n' \
    > "$repo/$INDEX"
  expect "an index whose point 4 does not name the page" 1 "that names $PAGE"
  echo "annex-ii: self-test passed."
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
