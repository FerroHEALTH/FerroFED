#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The technical documentation guard (#671, #1065), for Regulation (EU)
# 2025/327 Art 30(1)(c), Art 30(2), Art 37 and Annex III, and for Annex VII
# of Regulation (EU) 2024/2847, whose part the same set holds (EHDS Art
# 104(2)). How it checks is our own design: no specification governs it. It
# holds three things.
#
#   index    conformance/technical-documentation.tsv names, for every element
#            of Annex III and the Art 37(2) reference to the testing
#            environment, at least one place FerroFED documents it. Each
#            row's element is a phrase of the vendored Regulation, its target
#            is a file of the tree, its section is a heading of that file, a
#            partial or planned row names its issue, and the book page lists
#            every item.
#   cra      conformance/cra-technical-documentation.tsv does the same for
#            every point of CRA Annex VII, against the vendored CRA, and may
#            also mark a point not-applicable, with a target that says why;
#            the book page lists every point as `point <item>`.
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
readonly CRA_INDEX=conformance/cra-technical-documentation.tsv
readonly CRA_REGULATION=docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml
readonly CRA_ITEMS='1(a) 1(b) 1(c) 1(d) 2(a) 2(b) 2(c) 3 4 5 6 7 8'
readonly CHANGES_HEADING='Changes to the harmonised components'
readonly REVIEW_HEADING='Review and change'
# The harmonised software components (Art 25(1)) and the code that serves them.
readonly COMPONENTS=(
  app/ferrofed-eehrxf
  app/ferrofed-server/src/access
  app/ferrofed-server/src/fhir
)

# The harmonised components taken from crates.io, each pinned by a line of
# the root Cargo.toml: the logging component comes from the family's audit
# library, FerroHEALTH/FerroAUDIT (#1176), and the interoperability component
# from FerroHEALTH/FerroBRIDGE (#813). A moved pin is a changed component.
readonly PUBLISHED=(
  ehds-logging
  eehrxf
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

# regulation_text IN OUT: the vendored Regulation IN as one line of plain
# text, the markup and the no-break spaces turned into single spaces.
regulation_text() {
  local nbsp
  nbsp="$(printf '\302\240')"
  tr '\n\r\t' '   ' < "$1" |
    sed -e 's/<[^>]*>/ /g' -e "s/$nbsp/ /g" -e 's/&amp;/\&/g' -e 's/  */ /g' > "$2"
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

# check_index INDEX REGULATION ITEMS PREFIX WHAT NA: every row of INDEX held
# to REGULATION and the tree, every item of ITEMS with a row, and the book
# page listing each as `| PREFIX<item> |`. WHAT names an item in a refusal;
# NA is yes when a row may be not-applicable.
check_index() {
  local index="$1" regulation="$2" items="$3" prefix="$4" what="$5" na="$6"
  local text item element target section status issue line=0 seen statuses
  for f in "$index" "$PAGE" "$regulation"; do
    [[ -f "$f" ]] || {
      fail "$f is missing."
      return
    }
  done
  statuses="documented, partial or planned"
  [[ "$na" != yes ]] || statuses="documented, partial, planned or not-applicable"
  text="$(mktemp)"
  regulation_text "$regulation" "$text"
  seen=" "
  while IFS= read -r row; do
    line=$((line + 1))
    case "$row" in '#'* | '') continue ;; esac
    if [[ "$(printf '%s\n' "$row" | awk -F'\t' '{ print NF }')" != 6 ]]; then
      fail "$index line $line has not six tab-separated columns."
      continue
    fi
    IFS=$'\t' read -r item element target section status issue <<< "$row"
    if [[ "$item" == item ]]; then
      [[ "$row" == $'item\telement\ttarget\tsection\tstatus\tissue' ]] ||
        fail "$index header is not item, element, target, section, status, issue."
      continue
    fi
    case " $items " in
    *" $item "*) seen="$seen$item " ;;
    *) fail "$index line $line: $item is not $what." ;;
    esac
    grep -Fq -- "$element" "$text" ||
      fail "$index line $line: \"$element\" is not a phrase of $regulation."
    case "$target" in
    /* | *..*) fail "$index line $line: $target is not a path inside the tree." ;;
    *)
      if [[ ! -f "$target" ]]; then
        fail "$index line $line: $target does not exist."
      elif [[ "$section" != - ]] && ! has_heading "$target" "$section"; then
        fail "$index line $line: $target has no heading \"$section\"."
      fi
      ;;
    esac
    case "$status" in
    documented) [[ "$issue" == - || "$issue" =~ ^#[0-9]+$ ]] ||
      fail "$index line $line: issue \"$issue\" is neither - nor #N." ;;
    partial | planned) [[ "$issue" =~ ^#[0-9]+$ ]] ||
      fail "$index line $line: a $status row names its issue as #N." ;;
    not-applicable) [[ "$na" == yes ]] ||
      fail "$index line $line: status \"$status\" is not $statuses."
      [[ "$issue" == - ]] || fail "$index line $line: a not-applicable row names no issue." ;;
    *) fail "$index line $line: status \"$status\" is not $statuses." ;;
    esac
  done < "$index"
  rm -f -- "$text"
  for item in $items; do
    case "$seen" in
    *" $item "*) ;;
    *) fail "$index has no row for $item." ;;
    esac
    grep -Fq -- "| $prefix$item |" "$PAGE" || fail "$PAGE lists no row for $prefix$item."
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
  for component in "${PUBLISHED[@]}"; do
    grep -Fq -- "\`$component\`" "$PAGE" || fail "$PAGE does not name the published component $component."
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
  for component in "${PUBLISHED[@]}"; do
    if git diff "$tag" -- Cargo.toml | grep -Eq "^[-+]$component = "; then
      changed="${changed:+$changed
}Cargo.toml ($component)"
    fi
  done
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
  check_index "$INDEX" "$REGULATION" "$ITEMS" "" "an element of Annex III or Art 37(2)" no
  check_index "$CRA_INDEX" "$CRA_REGULATION" "$CRA_ITEMS" "point " "a point of CRA Annex VII" yes
  check_changes
  if [[ "$failures" -gt 0 ]]; then
    echo "technical-documentation: $failures refusals." >&2
    return 1
  fi
  echo "technical-documentation: the indexes cover Annex III and CRA Annex VII, and the change record is complete."
}

self_test() {
  local script status item
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  repo="$work/repo"
  mkdir -p "$repo/scripts/checks" "$repo/conformance" "$repo/docs/specs/eu-ehds" "$repo/docs/specs/eu-cra" \
    "$repo/website/book/src/evaluate" "$repo/app/ferrofed-eehrxf"
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
  # space, and the CRA's own, one index row per item of each, and the two
  # pages.
  fixture() {
    printf '<p>a phrase</p>\n<p>of the\302\240<span>Regulation</span></p>\n' > "$repo/$REGULATION"
    printf '<p>a phrase of the <span>Act</span></p>\n' > "$repo/$CRA_REGULATION"
    {
      printf '# a comment\n'
      printf 'item\telement\ttarget\tsection\tstatus\tissue\n'
      for item in $ITEMS; do
        printf '%s\tphrase of the Regulation\t%s\tThe forms\tdocumented\t-\n' "$item" "$PAGE"
      done
    } > "$repo/$INDEX"
    {
      printf 'item\telement\ttarget\tsection\tstatus\tissue\n'
      for item in $CRA_ITEMS; do
        printf '%s\tphrase of the Act\t%s\tThe forms\tdocumented\t-\n' "$item" "$PAGE"
      done
    } > "$repo/$CRA_INDEX"
    {
      printf '# Technical documentation\n\n## The forms\n\n| Item | Where |\n|---|---|\n'
      for item in $ITEMS; do printf '| %s | here |\n' "$item"; done
      printf '\n| Annex VII | Where |\n|---|---|\n'
      for item in $CRA_ITEMS; do printf '| point %s | here |\n' "$item"; done
      printf '\n## %s\n\n' "$CHANGES_HEADING"
      for component in "${COMPONENTS[@]}" "${PUBLISHED[@]}"; do printf -- '- \140%s\140\n' "$component"; done
      printf '\n| Release | Date |\n|---|---|\n| 0.0.1 | 2026-01-01 |\n'
    } > "$repo/$PAGE"
    printf '# Clinical safety\n\n## %s\n\n| Reviewed on | Release |\n|---|---|\n| 2026-01-01 | 0.0.1 |\n' \
      "$REVIEW_HEADING" > "$repo/$SAFETY"
    manifest 0.0.1 0.1.0
  }
  # manifest VERSION PIN: the root Cargo.toml at VERSION, with the published
  # logging component pinned at PIN.
  manifest() {
    printf '[workspace.package]\nversion = "%s"\n\n[workspace.dependencies]\nehds-logging = { version = "=%s" }\n' \
      "$1" "$2" > "$repo/Cargo.toml"
  }
  # row_in INDEX ITEM FIELD VALUE: set one field of ITEM's row of INDEX; a
  # FIELD of 0 drops it. row and cra_row name the index.
  row_in() {
    awk -F'\t' -v OFS='\t' -v item="$2" -v field="$3" -v value="${4:-}" '
      $1 == item && field == 0 { next }
      $1 == item { $field = value }
      { print }
    ' "$repo/$1" > "$work/index" && mv "$work/index" "$repo/$1"
  }
  row() { row_in "$INDEX" "$@"; }
  cra_row() { row_in "$CRA_INDEX" "$@"; }

  fixture
  printf 'one\n' > "$repo/app/ferrofed-eehrxf/lib.rs"
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
  row 6 5 'done'
  expect "an unknown status" 1 "status \"done\""
  fixture
  sed -i.orig '/^| 37(2) |/d' "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that lists no row for an item" 1 "lists no row for 37(2)"
  fixture
  sed -i.orig '/app\/ferrofed-eehrxf/d' "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that drops a component path" 1 "component path app/ferrofed-eehrxf"
  fixture
  sed -i.orig "/- \`ehds-logging\`/d" "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that drops a published component" 1 "published component ehds-logging"
  fixture
  row 3 5 not-applicable
  expect "a not-applicable Annex III row" 1 "status \"not-applicable\" is not documented, partial or planned"
  fixture
  cra_row '2(b)' 0
  expect "a CRA point with no row" 1 "cra-technical-documentation.tsv has no row for 2(b)"
  fixture
  cra_row 8 1 9
  expect "an unknown CRA point" 1 "9 is not a point of CRA Annex VII"
  fixture
  cra_row 4 2 'phrase of the Regulation'
  expect "an element the CRA does not hold" 1 "is not a phrase of $CRA_REGULATION"
  fixture
  cra_row '1(c)' 5 not-applicable
  expect "a not-applicable CRA row" 0
  cra_row '1(c)' 6 '#7'
  expect "a not-applicable CRA row with an issue" 1 "a not-applicable row names no issue"
  fixture
  cra_row 7 5 partial
  expect "a partial CRA row with no issue" 1 "a partial row names its issue"
  fixture
  sed -i.orig '/^| point 2(c) |/d' "$repo/$PAGE" && rm -f "$repo/$PAGE.orig"
  expect "a page that lists no row for a CRA point" 1 "lists no row for point 2(c)"
  fixture

  # Change control: the release v0.0.1, then a bump to 0.0.2.
  stub_git tag v0.0.1
  manifest 0.0.2 0.1.0
  expect "a bump that changes no component" 0 "no harmonised component changed between v0.0.1 and 0.0.2"
  manifest 0.0.2 0.1.1
  expect "a bump that moves a published component's pin with no row" 1 "has no row whose release is 0.0.2"
  manifest 0.0.2 0.1.0
  printf 'two\n' > "$repo/app/ferrofed-eehrxf/lib.rs"
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
  printf 'three\n' > "$repo/app/ferrofed-eehrxf/lib.rs"
  stub_git commit -q -a -m "a change after the release"
  expect "a routine change between releases" 0 "between v0.0.1 and 0.0.2"
  manifest 0.0.3-rc.1 0.1.0
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
