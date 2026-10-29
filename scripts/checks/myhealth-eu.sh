#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The MyHealth@EU self-assessment guard (#1423), for FerroFED's reading of
# the MyHealth@EU Requirements Catalogue v10.0.0. How it checks is our own
# design: no specification governs it.
#
# conformance/myhealth-eu.tsv has one row for every requirement id of the
# catalogue and one for its exception handling. The guard holds:
#
#   the ids       every id of the catalogue has exactly one row, under its
#                 own section number, and no other id has one;
#   the status    met, partly-met, waits-art-15-1, not-met, not-assessed or
#                 not-applicable; a met or not-applicable row names no
#                 issue, a waits-art-15-1 row names #1425, and every row
#                 states FerroFED's conclusion;
#   the tree      each evidence item is a file of the tree, and its anchor,
#                 when it has one, is a heading of a Markdown file or a test
#                 function of a Rust file;
#   the renders   the badge file, the README block and the book page block
#                 equal what scripts/conformance/myhealth-eu.sh renders, and
#                 the book summary lists the page;
#   no text       the catalogue is privileged-access: a conclusion is short,
#                 carries no quotation mark, no requirement verb and none of
#                 the catalogue's own terms, the page carries no quotation
#                 mark either, and no file of the tree is a copy of the
#                 catalogue or named as one.
#
# Usage:
#   scripts/checks/myhealth-eu.sh
#   scripts/checks/myhealth-eu.sh --self-test
# Needs awk, git, grep, sed and shasum or sha256sum. Exit 1 naming each
# refusal; 2 on a usage error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly MAPPING=conformance/myhealth-eu.tsv
readonly BADGE=conformance/myhealth-eu-badge.json
readonly README=README.md
readonly PAGE=website/book/src/evaluate/myhealth-eu.md
readonly SUMMARY=website/book/src/SUMMARY.md
readonly RENDER=scripts/conformance/myhealth-eu.sh
readonly HEADER=$'id\tsection\tstatus\tevidence\tissue\tnote'
readonly NOTE_MAX=300
# The catalogue's ids with their section numbers, read from its table of
# contents: identifiers and numbers only, never a title.
readonly IDS='01=1.9 01.01=1.9.1 01.02=1.9.2
02=1.10 02.01=1.10.1 02.01.01=1.10.1.1 02.01.02=1.10.1.2 02.01.03=1.10.1.3
03=1.11 03.01=1.11.1 03.02=1.11.2 03.03=1.11.3 03.04=1.11.4
04=1.12 04.01=1.12.1 04.02=1.12.2 04.03=1.12.3 04.04=1.12.4
05=1.13 05.01=1.13.1 05.02=1.13.2 05.03=1.13.3
06=1.14 06.01=1.14.1 06.02=1.14.2 06.03=1.14.3
07=1.15 07.01=1.15.1 07.02=1.15.2 07.03=1.15.3 07.04=1.15.4
08=1.16 08.01=1.16.1 08.02=1.16.2 08.03=1.16.3
09=1.17 09.01=1.17.1 09.01.01=1.17.1.1 09.02=1.17.2
10=1.18 10.01=1.18.1 10.02=1.18.2 10.03=1.18.3
11=1.19 11.01=1.19.1 11.01.01=1.19.1.1 11.01.02=1.19.1.2 11.01.03=1.19.1.3
11.01.04=1.19.1.4 11.01.04.01=1.19.1.4.1 11.01.04.02=1.19.1.4.2
11.02=1.19.2 11.03=1.19.3 11.04=1.19.4
12=1.20 12.01=1.20.1
13=1.21 13.01=1.21.1 13.02=1.21.2 13.03=1.21.3
14=1.22 14.01=1.22.1 14.02=1.22.2 14.03=1.22.3
15=1.23 15.01=1.23.1 15.02=1.23.2 15.03=1.23.3
16=1.24 16.01=1.24.1 16.02=1.24.2 16.03=1.24.3
1.25=1.25'
# The SHA-256 and byte size of the v10.0.0 PDF export, so a committed copy is
# refused without the guard holding any of its text.
readonly CATALOGUE_SHA256=4743869bbd57564a81b567c86619523f73f2ff0fabfec9a12f78127fd128408a
readonly CATALOGUE_BYTES=10005748
# A conclusion is FerroFED's own words: no requirement verb and none of the
# catalogue's own names for the parties, which FerroFED writes in lower case.
readonly REFUSED_VERBS='must|shall|should'
readonly REFUSED_TERMS='NCPeH|NCP-A|NCP-B|Country of affiliation|Country of treatment'
readonly QUOTES=$'"|\xe2\x80\x9c|\xe2\x80\x9d|\xe2\x80\x9e|\xc2\xab|\xc2\xbb|\xe2\x80\x98'

failures=0

fail() {
  echo "::error::myhealth-eu: $*" >&2
  failures=$((failures + 1))
}

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

has_heading() {
  awk -v want="$2" '
    /^```/ { fence = !fence; next }
    !fence && /^#+ / { h = $0; sub(/^#+ /, "", h); if (h == want) found = 1 }
    END { exit !found }
  ' "$1"
}

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

check_evidence() {
  local line="$1" rest="$2" item path anchor
  [[ "$rest" != - && -n "$rest" ]] || {
    fail "$MAPPING line $line names no evidence."
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
      fail "$MAPPING line $line: \"$path\" is not a path inside the tree."
      continue
      ;;
    esac
    if [[ ! -f "$path" ]]; then
      fail "$MAPPING line $line: $path does not exist."
      continue
    fi
    [[ -n "$anchor" ]] || continue
    case "$path" in
    *.md) has_heading "$path" "$anchor" ||
      fail "$MAPPING line $line: $path has no heading \"$anchor\"." ;;
    *.rs) has_test "$path" "$anchor" ||
      fail "$MAPPING line $line: $path has no test \"$anchor\"." ;;
    *) fail "$MAPPING line $line: $path takes no anchor; only a Markdown heading or a Rust test does." ;;
    esac
  done
}

# check_note LINE NOTE: FerroFED's conclusion, in its own words.
check_note() {
  local line="$1" note="$2"
  [[ -n "$note" && "$note" != - ]] || {
    fail "$MAPPING line $line states no conclusion in its note."
    return
  }
  [[ "${#note}" -le "$NOTE_MAX" ]] ||
    fail "$MAPPING line $line: the note is longer than $NOTE_MAX characters; state the conclusion, never the requirement."
  if printf '%s' "$note" | grep -Eq "$QUOTES"; then
    fail "$MAPPING line $line: the note carries a quotation mark; the catalogue is never quoted."
  fi
  if printf '%s' "$note" | grep -Eiwq "$REFUSED_VERBS" || printf '%s' "$note" | grep -Ewq "$REFUSED_TERMS"; then
    fail "$MAPPING line $line: the note uses a requirement verb or a catalogue term; write FerroFED's conclusion in its own words."
  fi
}

# sha256 FILE: the hex digest of FILE.
sha256() {
  if command -v sha256sum > /dev/null; then sha256sum "$1" | cut -c1-64; else shasum -a 256 "$1" | cut -c1-64; fi
}

# check_copies: no file of the tree is the catalogue or named as one.
check_copies() {
  local path
  while IFS= read -r path; do
    fail "$path is named as the Requirements Catalogue, which the repository never holds."
  done < <(git ls-files --cached --others --exclude-standard |
    grep -Ei 'requirements[^/]*catalog|ehdsi[^/]*requirement|myhealth[^/]*requirement' || true)
  while IFS= read -r -d '' path; do
    [[ "$(sha256 "$path")" == "$CATALOGUE_SHA256" ]] &&
      fail "${path#./} is a copy of the Requirements Catalogue, which the repository never holds."
  done < <(find . \( -name .git -o -name target -o -name node_modules \) -prune -o \
    -type f -size "${CATALOGUE_BYTES}c" -print0)
}

check_renders() {
  local fresh
  fresh="$(mktemp)"
  if bash "$RENDER" --badge > "$fresh" 2> /dev/null; then
    diff -q "$fresh" "$BADGE" > /dev/null 2>&1 ||
      fail "$BADGE is stale (run $RENDER --write)."
  else
    fail "$RENDER could not render the badge."
  fi
  if bash "$RENDER" --readme-block > "$fresh" 2> /dev/null; then
    sed -n '/^<!-- myhealth-eu:begin -->$/,/^<!-- myhealth-eu:end -->$/p' "$README" | diff -q "$fresh" - > /dev/null ||
      fail "the $README myhealth-eu block is stale (run $RENDER --write)."
  else
    fail "$RENDER could not render the README block."
  fi
  if bash "$RENDER" --page-block > "$fresh" 2> /dev/null; then
    sed -n '/^<!-- myhealth-eu:begin -->$/,/^<!-- myhealth-eu:end -->$/p' "$PAGE" | diff -q "$fresh" - > /dev/null ||
      fail "the $PAGE block is stale (run $RENDER --write)."
  else
    fail "$RENDER could not render the page block."
  fi
  rm -f -- "$fresh"
  if grep -Eq "$QUOTES" "$PAGE"; then
    fail "$PAGE carries a quotation mark; the catalogue is never quoted."
  fi
  grep -Fq '(evaluate/myhealth-eu.md)' "$SUMMARY" || fail "$SUMMARY does not list $PAGE."
}

run() {
  local row line=0 seen=" " id section status evidence issue note pair want
  for f in "$MAPPING" "$BADGE" "$README" "$PAGE" "$SUMMARY" "$RENDER"; do
    [[ -f "$f" ]] || {
      fail "$f is missing."
      finish
      return
    }
  done
  while IFS= read -r row; do
    line=$((line + 1))
    case "$row" in '#'* | '') continue ;; esac
    if [[ "$(printf '%s\n' "$row" | awk -F'\t' '{ print NF }')" != 6 ]]; then
      fail "$MAPPING line $line has not six tab-separated columns."
      continue
    fi
    IFS=$'\t' read -r id section status evidence issue note <<< "$row"
    if [[ "$id" == id ]]; then
      [[ "$row" == "$HEADER" ]] ||
        fail "$MAPPING header is not id, section, status, evidence, issue, note."
      continue
    fi
    want=""
    for pair in $IDS; do
      [[ "${pair%%=*}" == "$id" ]] && want="${pair#*=}"
    done
    if [[ -z "$want" ]]; then
      fail "$MAPPING line $line: $id is not a requirement id of the catalogue."
      continue
    fi
    [[ "$section" == "$want" ]] || fail "$MAPPING line $line: $id is section $want of the catalogue, not $section."
    case "$seen" in
    *" $id "*) fail "$MAPPING line $line: $id has a second row." ;;
    *) seen="$seen$id " ;;
    esac
    check_evidence "$line" "$evidence"
    check_note "$line" "$note"
    [[ "$issue" == - || "$issue" =~ ^#[0-9]+( #[0-9]+)*$ ]] ||
      fail "$MAPPING line $line: the issue is - or #N, several separated by one space."
    case "$status" in
    met | not-applicable)
      [[ "$issue" == - ]] || fail "$MAPPING line $line: a $status row names no issue."
      ;;
    waits-art-15-1)
      [[ " $issue " == *" #1425 "* ]] || fail "$MAPPING line $line: a waits-art-15-1 row names #1425."
      ;;
    partly-met | not-met | not-assessed) ;;
    *) fail "$MAPPING line $line: status \"$status\" is not met, partly-met, waits-art-15-1, not-met, not-assessed or not-applicable." ;;
    esac
  done < "$MAPPING"
  for pair in $IDS; do
    case "$seen" in
    *" ${pair%%=*} "*) ;;
    *) fail "$MAPPING has no row for ${pair%%=*}." ;;
    esac
  done
  check_renders
  check_copies
  finish
}

finish() {
  if [[ "$failures" -gt 0 ]]; then
    echo "myhealth-eu: $failures refusals." >&2
    exit 1
  fi
  echo "myhealth-eu: every catalogue id has its row, its evidence and its rendering, and no catalogue text is held."
}

self_test() {
  local script render status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  render="$(cd "$(dirname "${BASH_SOURCE[0]}")/../conformance" && pwd)/myhealth-eu.sh"
  repo="$work/repo"
  flunk() {
    echo "myhealth-eu: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  expect() {
    local name="$1" want="$2" pattern="${3:-}"
    status=0
    (cd "$repo" && bash scripts/checks/myhealth-eu.sh) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
    [[ -z "$pattern" ]] || grep -Fq -- "$pattern" "$work/out" || flunk "$name did not say \"$pattern\"."
  }
  # The fixture: every id met on one page, the renders written, a page,
  # a summary, a heading and a test, in a git repository of its own.
  fixture() {
    rm -rf -- "$repo"
    mkdir -p "$repo/scripts/checks" "$repo/scripts/conformance" "$repo/conformance" \
      "$repo/docs" "$repo/website/book/src/evaluate" "$repo/tests"
    cp "$script" "$repo/scripts/checks/myhealth-eu.sh"
    cp "$render" "$repo/scripts/conformance/myhealth-eu.sh"
    printf '# Evidence\n\n## The evidence\n' > "$repo/docs/evidence.md"
    printf '#[test]\nfn a_test() {}\n\nfn not_a_test() {}\n' > "$repo/tests/it.rs"
    {
      printf '# a comment\n%s\n' "$HEADER"
      for pair in $IDS; do
        printf '%s\t%s\tmet\tdocs/evidence.md; docs/evidence.md::The evidence; tests/it.rs::a_test\t-\tThe gateway does its share.\n' \
          "${pair%%=*}" "${pair#*=}"
      done
    } > "$repo/$MAPPING"
    printf '# MyHealth@EU\n\n<!-- myhealth-eu:begin -->\n<!-- myhealth-eu:end -->\n' > "$repo/$PAGE"
    printf '# FerroFED\n\n<!-- myhealth-eu:begin -->\n<!-- myhealth-eu:end -->\n' > "$repo/$README"
    printf -- '- [MyHealth@EU](evaluate/myhealth-eu.md)\n' > "$repo/$SUMMARY"
    (cd "$repo" && bash scripts/conformance/myhealth-eu.sh --write > /dev/null &&
      git init -q . && git add -A)
  }
  # row ID FIELD VALUE: set one field of ID's row; a FIELD of 0 drops it.
  row() {
    awk -F'\t' -v OFS='\t' -v id="$1" -v field="$2" -v value="${3:-}" '
      $1 == id && field == 0 { next }
      $1 == id { $field = value }
      { print }
    ' "$repo/$MAPPING" > "$work/mapping" && mv "$work/mapping" "$repo/$MAPPING"
  }
  rerender() {
    (cd "$repo" && bash scripts/conformance/myhealth-eu.sh --write > /dev/null)
  }

  fixture
  expect "a complete mapping" 0 "every catalogue id has its row"

  row 07.04 0
  rerender
  expect "an id with no row" 1 "has no row for 07.04"
  fixture
  awk -F'\t' '$1 == "12"' "$repo/$MAPPING" > "$work/twelve"
  cat "$work/twelve" >> "$repo/$MAPPING"
  rerender
  expect "an id with two rows" 1 "12 has a second row"
  fixture
  row 03 1 '17'
  rerender
  expect "an id the catalogue does not have" 1 "17 is not a requirement id"
  fixture
  row 05.02 2 '1.13.3'
  rerender
  expect "an id under another section" 1 "05.02 is section 1.13.2 of the catalogue, not 1.13.3"
  fixture
  row 06 3 compliant
  rerender
  expect "an unknown status" 1 "status \"compliant\""
  fixture
  row 06 5 '#12'
  rerender
  expect "a met row naming an issue" 1 "a met row names no issue"
  fixture
  row 06 3 waits-art-15-1
  row 06 5 '#12'
  rerender
  expect "a waiting row without the act's issue" 1 "names #1425"
  row 06 5 '#12 #1425'
  rerender
  expect "a waiting row with the act's issue" 0
  fixture
  row 06 3 not-met
  row 06 5 '#12,#13'
  rerender
  expect "issues not separated by one space" 1 "the issue is - or #N"
  fixture
  row 06 6 -
  rerender
  expect "a row with no conclusion" 1 "states no conclusion"
  fixture
  row 06 6 'The gateway answers "as written" by the catalogue.'
  rerender
  expect "a quoted note" 1 "carries a quotation mark"
  fixture
  row 06 6 $'The gateway answers \xe2\x80\x9cas written\xe2\x80\x9d.'
  rerender
  expect "a note in curly quotes" 1 "carries a quotation mark"
  fixture
  row 06 6 'The national infrastructure shall make it available.'
  rerender
  expect "a requirement verb" 1 "requirement verb or a catalogue term"
  fixture
  row 06 6 'The NCPeH answers.'
  rerender
  expect "a catalogue term" 1 "requirement verb or a catalogue term"
  fixture
  row 06 6 "$(printf 'x%.0s' $(seq 1 301))"
  rerender
  expect "a long note" 1 "longer than 300 characters"
  fixture
  row 06 4 missing.md
  rerender
  expect "evidence that does not exist" 1 "missing.md does not exist"
  fixture
  row 06 4 'tests/it.rs::not_a_test'
  rerender
  expect "a function that is no test" 1 "has no test \"not_a_test\""
  fixture
  row 06 4 "docs/evidence.md::No such heading"
  rerender
  expect "a heading that does not exist" 1 "has no heading \"No such heading\""
  fixture
  row 06 3 partly-met
  expect "a badge and blocks not rendered again" 1 "is stale"
  fixture
  printf 'The catalogue says "x".\n' >> "$repo/$PAGE"
  expect "a quotation on the page" 1 "$PAGE carries a quotation mark"
  fixture
  printf '\n' > "$repo/$SUMMARY"
  expect "a page the summary does not list" 1 "does not list"
  fixture
  printf 'x' > "$repo/docs/eHDSI Requirements Catalogue v10.0.0.txt"
  expect "a file named as the catalogue" 1 "is named as the Requirements Catalogue"
  echo "myhealth-eu: self-test passed."
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
