#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The technical documentation guard (#671), for Regulation (EU) 2025/327 Art
# 30(1)(c), Art 30(2), Art 37 and Annex III. How it checks is our own design:
# no specification governs it. It holds two things.
#
#   index    conformance/technical-documentation.tsv names, for every element
#            of Annex III and the Art 37(2) reference to the testing
#            environment, at least one place FerroFED documents it. Each
#            row's element is a phrase of the vendored Regulation, its target
#            is a file of the tree, its section is a heading of that file, a
#            partial or planned row names its issue, and the book page lists
#            every item.
#   changes  a version whose harmonised components differ from the previous
#            release has a row for that version in the review table of the
#            clinical safety risk file and in the change table of the book
#            page (Art 30(2)). The version is the root Cargo.toml
#            [workspace.package] version; the previous release is the newest
#            vX.Y.Z tag below it, read locally and from origin when origin
#            answers, and fetched alone when it is not local. Between
#            releases the version is the last release's, whose rows exist, so
#            the version bump and the tag cut from it are what must add them.
#
# Usage:
#   scripts/checks/technical-documentation.sh
#   scripts/checks/technical-documentation.sh --self-test
# Needs git, awk, sed, sort and tr. Exit 1 naming each refusal; 2 on a usage
# error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly INDEX=conformance/technical-documentation.tsv
readonly PAGE=website/book/src/evaluate/technical-documentation.md
readonly SAFETY=website/book/src/evaluate/clinical-safety.md
readonly REGULATION=docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml
readonly ITEMS='1(a) 1(b) 1(c) 1(d) 1(e) 1(f) 1(g) 1(h) 1(i) 1(j) 2 3 4 5 6 37(2)'
readonly CHANGES_HEADING='Changes to the harmonised components'
readonly REVIEW_HEADING='Review and change'
# The harmonised software components (Art 25(1)) and the code that serves them.
readonly COMPONENTS=(
  crates/ehds-logging
  crates/eehrxf
  app/ferrofed-eehrxf
  app/ferrofed-server/src/access
  app/ferrofed-server/src/fhir
)

failures=0

fail() {
  echo "::error::technical-documentation: $*" >&2
  failures=$((failures + 1))
}

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

# regulation_text OUT: the vendored Regulation as one line of plain text, the
# markup and the no-break spaces turned into single spaces.
regulation_text() {
  local nbsp
  nbsp="$(printf '\302\240')"
  tr '\n\r\t' '   ' < "$REGULATION" |
    sed -e 's/<[^>]*>/ /g' -e "s/$nbsp/ /g" -e 's/&amp;/\&/g' -e 's/  */ /g' > "$1"
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

# table_has FILE HEADING CELL VALUE: a table row under the `## HEADING` of
# FILE whose CELLth cell, trimmed, is VALUE.
table_has() {
  awk -v heading="## $2" -v cell="$3" -v want="$4" '
    $0 == heading { inside = 1; next }
    inside && /^## / { inside = 0 }
    inside && /^\|/ {
      n = split($0, c, "|")
      if (cell + 1 <= n) {
        v = c[cell + 1]
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
        if (v == want) found = 1
      }
    }
    END { exit !found }
  ' "$1"
}

check_index() {
  local text item element target section status issue line=0 seen
  for f in "$INDEX" "$PAGE" "$REGULATION"; do
    [[ -f "$f" ]] || {
      fail "$f is missing."
      return
    }
  done
  text="$(mktemp)"
  regulation_text "$text"
  seen=" "
  while IFS= read -r row; do
    line=$((line + 1))
    case "$row" in '#'* | '') continue ;; esac
    if [[ "$(printf '%s\n' "$row" | awk -F'\t' '{ print NF }')" != 6 ]]; then
      fail "$INDEX line $line has not six tab-separated columns."
      continue
    fi
    IFS=$'\t' read -r item element target section status issue <<< "$row"
    if [[ "$item" == item ]]; then
      [[ "$row" == $'item\telement\ttarget\tsection\tstatus\tissue' ]] ||
        fail "$INDEX header is not item, element, target, section, status, issue."
      continue
    fi
    case " $ITEMS " in
    *" $item "*) seen="$seen$item " ;;
    *) fail "$INDEX line $line: $item is not an element of Annex III or Art 37(2)." ;;
    esac
    grep -Fq -- "$element" "$text" ||
      fail "$INDEX line $line: \"$element\" is not a phrase of $REGULATION."
    case "$target" in
    /* | *..*) fail "$INDEX line $line: $target is not a path inside the tree." ;;
    *)
      if [[ ! -f "$target" ]]; then
        fail "$INDEX line $line: $target does not exist."
      elif [[ "$section" != - ]] && ! has_heading "$target" "$section"; then
        fail "$INDEX line $line: $target has no heading \"$section\"."
      fi
      ;;
    esac
    case "$status" in
    documented) [[ "$issue" == - || "$issue" =~ ^#[0-9]+$ ]] ||
      fail "$INDEX line $line: issue \"$issue\" is neither - nor #N." ;;
    partial | planned) [[ "$issue" =~ ^#[0-9]+$ ]] ||
      fail "$INDEX line $line: a $status row names its issue as #N." ;;
    *) fail "$INDEX line $line: status \"$status\" is not documented, partial or planned." ;;
    esac
  done < "$INDEX"
  rm -f -- "$text"
  for item in $ITEMS; do
    case "$seen" in
    *" $item "*) ;;
    *) fail "$INDEX has no row for $item." ;;
    esac
    grep -Fq -- "| $item |" "$PAGE" || fail "$PAGE lists no row for $item."
  done
}

# workspace_version: the root Cargo.toml [workspace.package] version.
workspace_version() {
  awk '
    /^[[:space:]]*\[/ { h = $0; gsub(/[[:space:]]/, "", h); f = (h == "[workspace.package]"); next }
    f && /^[[:space:]]*version[[:space:]]*=/ {
      if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit }
    }
  ' Cargo.toml
}

# previous_release VERSION: the newest vX.Y.Z tag below VERSION's release
# part, local or at origin, fetched when it is not local; nothing when none.
previous_release() {
  local base="${1%%-*}" tag found=""
  while IFS= read -r tag; do
    [[ "${tag#v}" != "$base" ]] || continue
    [[ "$(printf '%s\n%s\n' "${tag#v}" "$base" | sort -V | head -n1)" == "${tag#v}" ]] || continue
    found="$tag"
  done < <({
    git tag --list 'v[0-9]*'
    git ls-remote --tags --refs origin 'v*' 2> /dev/null | sed -nE 's|.*refs/tags/||p' || true
  } | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -u | sort -V)
  [[ -n "$found" ]] || return 0
  if ! git rev-parse -q --verify "refs/tags/$found" > /dev/null; then
    if [[ "$(git rev-parse --is-shallow-repository)" == true ]]; then
      git fetch --quiet --no-tags --depth=1 origin "refs/tags/$found:refs/tags/$found"
    else
      git fetch --quiet --no-tags origin "refs/tags/$found:refs/tags/$found"
    fi
  fi
  printf '%s' "$found"
}

check_changes() {
  local version tag changed component
  for component in "${COMPONENTS[@]}"; do
    grep -Fq -- "\`$component\`" "$PAGE" || fail "$PAGE does not name the component path $component."
  done
  version="$(workspace_version)"
  [[ -n "$version" ]] || {
    fail "the root Cargo.toml declares no [workspace.package] version."
    return
  }
  tag="$(previous_release "$version")"
  if [[ -z "$tag" ]]; then
    echo "technical-documentation: no release before $version; no change to compare."
    return
  fi
  changed="$(git diff --name-only "$tag" -- "${COMPONENTS[@]}")"
  if [[ -z "$changed" ]]; then
    echo "technical-documentation: no harmonised component changed between $tag and $version."
    return
  fi
  echo "technical-documentation: $(printf '%s\n' "$changed" | wc -l | tr -d ' ') files of the harmonised components changed between $tag and $version."
  table_has "$SAFETY" "$REVIEW_HEADING" 2 "$version" ||
    fail "$version changes a harmonised component since $tag, and the review table of $SAFETY has no row whose release is $version (docs/release.md, Before the tag)."
  table_has "$PAGE" "$CHANGES_HEADING" 1 "$version" ||
    fail "$version changes a harmonised component since $tag, and \"$CHANGES_HEADING\" in $PAGE has no row for $version (docs/release.md, Before the tag)."
}

run() {
  check_index
  check_changes
  if [[ "$failures" -gt 0 ]]; then
    echo "technical-documentation: $failures refusals." >&2
    return 1
  fi
  echo "technical-documentation: the index covers Annex III and the change record is complete."
}

self_test() {
  local script status item
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  repo="$work/repo"
  mkdir -p "$repo/scripts/checks" "$repo/conformance" "$repo/docs/specs/eu-ehds" \
    "$repo/website/book/src/evaluate" "$repo/crates/ehds-logging"
  cp "$script" "$repo/scripts/checks/technical-documentation.sh"
  stub_git() {
    git -C "$repo" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c tag.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  flunk() {
    echo "technical-documentation: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  expect() {
    local name="$1" want="$2" pattern="${3:-}"
    status=0
    (cd "$repo" && bash scripts/checks/technical-documentation.sh) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
    [[ -z "$pattern" ]] || grep -Fq -- "$pattern" "$work/out" || flunk "$name did not say \"$pattern\"."
  }
  # The fixture: a Regulation whose phrase is split by markup and a no-break
  # space, one index row per item, and the two pages.
  fixture() {
    printf '<p>a phrase</p>\n<p>of the\302\240<span>Regulation</span></p>\n' > "$repo/$REGULATION"
    {
      printf '# a comment\n'
      printf 'item\telement\ttarget\tsection\tstatus\tissue\n'
      for item in $ITEMS; do
        printf '%s\tphrase of the Regulation\t%s\tThe forms\tdocumented\t-\n' "$item" "$PAGE"
      done
    } > "$repo/$INDEX"
    {
      printf '# Technical documentation\n\n## The forms\n\n| Item | Where |\n|---|---|\n'
      for item in $ITEMS; do printf '| %s | here |\n' "$item"; done
      printf '\n## %s\n\n' "$CHANGES_HEADING"
      for component in "${COMPONENTS[@]}"; do printf -- '- \140%s\140\n' "$component"; done
      printf '\n| Release | Date |\n|---|---|\n| 0.0.1 | 2026-01-01 |\n'
    } > "$repo/$PAGE"
    printf '# Clinical safety\n\n## %s\n\n| Reviewed on | Release |\n|---|---|\n| 2026-01-01 | 0.0.1 |\n' \
      "$REVIEW_HEADING" > "$repo/$SAFETY"
    printf '[workspace.package]\nversion = "0.0.1"\n' > "$repo/Cargo.toml"
  }
  # row ITEM FIELD VALUE: set one field of ITEM's row; a FIELD of 0 drops it.
  row() {
    awk -F'\t' -v OFS='\t' -v item="$1" -v field="$2" -v value="${3:-}" '
      $1 == item && field == 0 { next }
      $1 == item { $field = value }
      { print }
    ' "$repo/$INDEX" > "$work/index" && mv "$work/index" "$repo/$INDEX"
  }

  fixture
  printf 'one\n' > "$repo/crates/ehds-logging/lib.rs"
  stub_git init -q -b main
  stub_git add -A
  stub_git commit -q -m "the first release"
  expect "a complete index with no earlier release" 0 "no release before 0.0.1"

  row '1(c)' 0
  expect "an item with no row" 1 "has no row for 1(c)"
  fixture
  row '1(d)' 1 '1(z)'
  expect "an unknown item" 1 "1(z) is not an element"
  fixture
  row 2 2 'phrase of a Regulation'
  expect "an element the Regulation does not hold" 1 "is not a phrase of"
  fixture
  row 3 3 missing.md
  expect "a target that does not exist" 1 "missing.md does not exist"
  fixture
  row 3 3 ../outside.md
  expect "a target outside the tree" 1 "is not a path inside the tree"
  fixture
  row 4 4 'No such heading'
  expect "a section that is no heading" 1 "has no heading \"No such heading\""
  fixture
  row 5 5 planned
  expect "a planned row with no issue" 1 "a planned row names its issue"
  row 5 6 '#12'
  expect "a planned row with its issue" 0
  fixture
  row 6 5 done
  expect "an unknown status" 1 "status \"done\""
  fixture
  sed -i.orig '/^| 37(2) |/d' "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that lists no row for an item" 1 "lists no row for 37(2)"
  fixture
  sed -i.orig '/crates\/eehrxf/d' "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that drops a component path" 1 "component path crates/eehrxf"
  fixture

  # Change control: the release v0.0.1, then a bump to 0.0.2.
  stub_git tag v0.0.1
  printf '[workspace.package]\nversion = "0.0.2"\n' > "$repo/Cargo.toml"
  expect "a bump that changes no component" 0 "no harmonised component changed between v0.0.1 and 0.0.2"
  printf 'two\n' > "$repo/crates/ehds-logging/lib.rs"
  stub_git commit -q -a -m "a component change"
  expect "a bump that changes a component with no row" 1 "has no row whose release is 0.0.2"
  printf '| 2026-02-01 | before 0.0.2 |\n' >> "$repo/$SAFETY"
  expect "a review row that is not the release's own" 1 "has no row whose release is 0.0.2"
  printf '| 2026-02-01 | 0.0.2 |\n' >> "$repo/$SAFETY"
  expect "a review row with no change row" 1 "has no row for 0.0.2"
  printf '| 0.0.2 | 2026-02-01 |\n' >> "$repo/$PAGE"
  expect "a bump with both rows" 0 "the change record is complete"

  # Between releases: the version is the last release's, and a component
  # change after its tag is the next bump's to record.
  stub_git commit -q -a -m "the second release"
  stub_git tag v0.0.2
  printf 'three\n' > "$repo/crates/ehds-logging/lib.rs"
  stub_git commit -q -a -m "a change after the release"
  expect "a routine change between releases" 0 "between v0.0.1 and 0.0.2"
  printf '[workspace.package]\nversion = "0.0.3-rc.1"\n' > "$repo/Cargo.toml"
  expect "a pre-release bump with no row" 1 "has no row whose release is 0.0.3-rc.1"
  echo "technical-documentation: self-test passed."
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
