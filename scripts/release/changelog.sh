#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Changelog fragments (no specification governs this: our own design). Every
# change with a user-visible effect adds one file under changelog.d/, so no two
# pull requests edit the same lines of CHANGELOG.md, and the release cut
# assembles the fragments into CHANGELOG.md, which follows Keep a Changelog
# 1.1.0 (https://keepachangelog.com/en/1.1.0/).
#
# A fragment is changelog.d/<issue>-<kebab-slug>.<section>.md, where <section>
# is upgrade, added, changed, deprecated, removed, fixed or security, and its
# content is one or more Markdown list items exactly as they read in CHANGELOG.md: a line
# that opens with "- ", continued by lines indented by at least two spaces.
# An upgrade fragment says what an operator must change to upgrade, and is
# assembled under "### Upgrade notes", first in its release: a section of our
# own beside the six Keep a Changelog defines.
# changelog.d/README.md is the format note and is never read as a fragment.
#
#   changelog.sh --check
#       Validates every fragment's name and content. Exit 0 when every one
#       holds, 1 otherwise, naming each defect.
#   changelog.sh --assemble <version> <date>
#       Writes a new "## [<version>] - <date>" section under a fresh, empty
#       [Unreleased], holding the entries still under [Unreleased] followed by
#       the fragments, section by section, Upgrade notes first and then the Keep a
#       Changelog order; moves
#       the [Unreleased] link reference on and adds the version's; then
#       `git rm`s the fragments. A release without a pre-release suffix also
#       gets its support period: the section opens with the end date, five
#       years from <date>, and SECURITY.md gets the version's row in its
#       support table (Regulation (EU) 2024/2847 Art 13(8), (19)); the end
#       date is printed. Exit 1 on any defect, with nothing written.
#   changelog.sh --self-test
#       Proves both modes over a stub repository.
#
# The output is deterministic: fragments are read by section in the order
# above, then by file name in the C locale. Exit 2 on a usage error.
set -euo pipefail
export LC_ALL=C

# The sections, in the order a release lists them: the Upgrade notes an
# operator reads before anything else (our own section), then the Keep a
# Changelog 1.1.0 ones.
readonly SECTIONS=(upgrade added changed deprecated removed fixed security)
readonly FRAGMENT_DIR=changelog.d
readonly NAME_RE='^[0-9]+-[a-z0-9]+(-[a-z0-9]+)*\.(upgrade|added|changed|deprecated|removed|fixed|security)\.md$'
# The support period of a release, in years from its release date: the minimum
# of Regulation (EU) 2024/2847 Art 13(8), third subparagraph. A longer period
# changes this number, SECURITY.md and docs/release.md together.
readonly SUPPORT_YEARS=5
readonly SUPPORT_MARKER='<!-- support periods: rows above this line -->'

die() {
  echo "changelog: $*" >&2
  exit 1
}

usage() {
  echo "usage: changelog.sh --check | --assemble <version> <date> | --self-test" >&2
  exit 2
}

# title SECTION: the heading a section carries in CHANGELOG.md.
title() {
  local section="$1"
  if [[ "$section" == upgrade ]]; then
    printf 'Upgrade notes'
    return
  fi
  printf '%s%s' "$(tr '[:lower:]' '[:upper:]' <<<"${section:0:1}")" "${section:1}"
}

# support_end DATE: the last day of the support period of a release made on
# DATE (YYYY-MM-DD), the same month and day SUPPORT_YEARS later. 29 February
# becomes 1 March, so the period is never shorter than SUPPORT_YEARS years (no
# specification governs the day: our own design).
support_end() {
  local date="$1" year
  year=$((10#${date:0:4} + SUPPORT_YEARS))
  if [[ "${date:5:5}" == 02-29 ]]; then
    printf '%04d-03-01' "$year"
    return
  fi
  printf '%04d-%s' "$year" "${date:5:5}"
}

# content_defect FILE: prints why FILE is no list item, or nothing when it is.
content_defect() {
  local file="$1"
  awk '
    { lines[NR] = $0 }
    END {
      last = NR
      while (last > 0 && lines[last] ~ /^[[:space:]]*$/) last--
      if (last == 0) { print "it is empty"; exit }
      if (lines[1] !~ /^- [^[:space:]]/) { print "line 1 does not open a list item with \"- \""; exit }
      for (i = 2; i <= last; i++) {
        if (lines[i] ~ /^- [^[:space:]]/ || lines[i] ~ /^  +[^[:space:]]/) continue
        if (lines[i] ~ /^[[:space:]]*$/) { print "line " i " is blank inside the entry"; exit }
        print "line " i " neither opens a list item nor continues one indented by two spaces"
        exit
      }
    }
  ' "$file"
}

# check: every fragment's name and content, every defect named.
check() {
  local fail=0 count=0 path name defect
  [[ -d "$FRAGMENT_DIR" ]] || die "$FRAGMENT_DIR/ is missing"
  for path in "$FRAGMENT_DIR"/* "$FRAGMENT_DIR"/.[!.]*; do
    [[ -e "$path" ]] || continue
    name="${path#"$FRAGMENT_DIR"/}"
    [[ "$name" != README.md ]] || continue
    if [[ ! -f "$path" ]]; then
      echo "changelog: $path is not a file; a fragment is one file directly under $FRAGMENT_DIR/." >&2
      fail=1
      continue
    fi
    if [[ ! "$name" =~ $NAME_RE ]]; then
      echo "changelog: $path is not named <issue>-<kebab-slug>.<section>.md with <section> one of ${SECTIONS[*]}." >&2
      fail=1
      continue
    fi
    defect="$(content_defect "$path")"
    if [[ -n "$defect" ]]; then
      echo "changelog: $path is not a Markdown list item: $defect." >&2
      fail=1
      continue
    fi
    count=$((count + 1))
  done
  [[ "$fail" -eq 0 ]] || exit 1
  echo "changelog: $count fragment(s) under $FRAGMENT_DIR/, every one well formed."
}

# assemble VERSION DATE: the cut, written to CHANGELOG.md in one move.
assemble() {
  local version="$1" date="$2" work start end total section file prev base out
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "'$version' is not a version such as 0.0.9 or 0.0.9-rc.1"
  [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || die "'$date' is not a date written YYYY-MM-DD"
  [[ -f CHANGELOG.md ]] || die "CHANGELOG.md is missing"
  check > /dev/null

  if awk -v heading="## [$version]" 'index($0, heading) == 1 { found = 1 } END { exit !found }' CHANGELOG.md; then
    die "CHANGELOG.md already has a [$version] section"
  fi
  start="$(grep -n -m1 -E '^## \[Unreleased\][[:space:]]*$' CHANGELOG.md | cut -d: -f1 || true)"
  [[ -n "$start" ]] || die "CHANGELOG.md has no '## [Unreleased]' heading"
  total="$(wc -l < CHANGELOG.md | tr -d ' ')"
  # The [Unreleased] body ends at the next version heading or at the link
  # reference block, whichever comes first.
  end="$(awk -v s="$start" 'NR > s && (/^## \[/ || /^\[[^]]+\]: /) { print NR - 1; exit }' CHANGELOG.md)"
  [[ -n "$end" ]] || end="$total"

  prev="$(sed -nE 's|^\[Unreleased\]: (.*)/compare/([^/]+)\.\.\.HEAD$|\2|p' CHANGELOG.md)"
  base="$(sed -nE 's|^\[Unreleased\]: (.*)/compare/([^/]+)\.\.\.HEAD$|\1|p' CHANGELOG.md)"
  [[ -n "$prev" && -n "$base" ]] || die "CHANGELOG.md has no '[Unreleased]: <repository>/compare/<tag>...HEAD' link reference"

  # A release placed on the market gets its support period; a pre-release
  # rehearses the lane and gets none.
  local support_end_date=""
  if [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    [[ -f SECURITY.md ]] || die "SECURITY.md is missing, so v$version has no support table to be listed in"
    grep -q -F -x "$SUPPORT_MARKER" SECURITY.md || die "SECURITY.md has no support table ending in the line '$SUPPORT_MARKER'"
    if grep -q -F "| v$version |" SECURITY.md; then
      die "SECURITY.md already lists the support period of v$version"
    fi
    support_end_date="$(support_end "$date")"
  fi

  work="$(mktemp -d)"
  # Global, so the EXIT trap still sees it after the function returns.
  assemble_work="$work"
  trap 'rm -r -- "$assemble_work"' EXIT

  # The entries still under [Unreleased], one file per section. Anything
  # outside a known section heading is refused, never dropped.
  sed -n "$((start + 1)),${end}p" CHANGELOG.md | awk -v dir="$work" '
    /^### / {
      heading = substr($0, 5)
      sub(/[[:space:]]+$/, "", heading)
      section = tolower(heading)
      if (section == "upgrade notes") section = "upgrade"
      if (section !~ /^(upgrade|added|changed|deprecated|removed|fixed|security)$/) {
        print "changelog: [Unreleased] has a section \"" heading "\", which is neither Upgrade notes nor a Keep a Changelog 1.1.0 section." > "/dev/stderr"
        exit 1
      }
      next
    }
    section == "" && /[^[:space:]]/ {
      print "changelog: [Unreleased] holds text outside a section heading: " $0 > "/dev/stderr"
      exit 1
    }
    section != "" { print > (dir "/old." section) }
  ' || exit 1

  # Each section of the release: the [Unreleased] entries first, in their
  # order, then the fragments by file name.
  local fragments=() entries=0
  for section in "${SECTIONS[@]}"; do
    {
      [[ ! -f "$work/old.$section" ]] || trim "$work/old.$section"
      for file in "$FRAGMENT_DIR"/*."$section".md; do
        [[ -e "$file" ]] || continue
        git ls-files --error-unmatch -- "$file" > /dev/null 2>&1 || die "$file is not tracked by git; commit it before the cut"
        fragments+=("$file")
        trim "$file"
      done
    } > "$work/new.$section"
    [[ ! -s "$work/new.$section" ]] || entries=$((entries + 1))
  done
  [[ "$entries" -gt 0 ]] || die "[$version] would be empty: no fragment under $FRAGMENT_DIR/ and no entry under [Unreleased]"

  out="$work/CHANGELOG.md"
  {
    sed -n "1,${start}p" CHANGELOG.md
    printf '\n## [%s] - %s\n' "$version" "$date"
    if [[ -n "$support_end_date" ]]; then
      printf '\nSupported until %s, %s years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)). A security fix ships in the latest release, as [SECURITY.md](%s/blob/main/SECURITY.md#supported-versions) says.\n' \
        "$support_end_date" "$SUPPORT_YEARS" "$base"
    fi
    for section in "${SECTIONS[@]}"; do
      [[ -s "$work/new.$section" ]] || continue
      printf '\n### %s\n\n' "$(title "$section")"
      cat "$work/new.$section"
    done
    printf '\n'
    if [[ "$end" -lt "$total" ]]; then
      sed -n "$((end + 1)),${total}p" CHANGELOG.md | awk -v ver="$version" -v prev="$prev" -v base="$base" '
        /^\[Unreleased\]: / {
          print "[Unreleased]: " base "/compare/v" ver "...HEAD"
          print "[" ver "]: " base "/compare/" prev "...v" ver
          next
        }
        { print }
      '
    fi
  } > "$out"

  if [[ -n "$support_end_date" ]]; then
    awk -v row="| v$version | $date | $support_end_date |" -v marker="$SUPPORT_MARKER" \
      '$0 == marker { print row } { print }' SECURITY.md > "$work/SECURITY.md"
  fi

  cat "$out" > CHANGELOG.md
  if [[ -n "$support_end_date" ]]; then
    cat "$work/SECURITY.md" > SECURITY.md
  fi
  if [[ "${#fragments[@]}" -gt 0 ]]; then
    git rm -q -- ${fragments[@]+"${fragments[@]}"}
  fi
  echo "changelog: [$version] - $date assembled from ${#fragments[@]} fragment(s) and the [Unreleased] entries; review CHANGELOG.md and commit."
  if [[ -n "$support_end_date" ]]; then
    echo "changelog: v$version is supported until $support_end_date ($SUPPORT_YEARS years from $date); its row is added to SECURITY.md."
  else
    echo "changelog: $version is a pre-release, so it gets no support period."
  fi
}

# trim FILE: FILE without its leading and trailing blank lines.
trim() {
  local file="$1"
  awk '
    { lines[NR] = $0 }
    END {
      first = 1
      while (first <= NR && lines[first] ~ /^[[:space:]]*$/) first++
      last = NR
      while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
      for (i = first; i <= last; i++) print lines[i]
    }
  ' "$file"
}

self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  # git writes its objects read-only, so the tree is made writable before it
  # is removed.
  trap 'chmod -R u+w "$work" && rm -r -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$work/scripts/release" "$work/$FRAGMENT_DIR"
  cp "$script" "$work/scripts/release/changelog.sh"

  # A repository of its own, so no global hook, signing key or identity of the
  # caller's reaches it.
  stub_git() {
    git -C "$work" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  flunk() {
    echo "changelog: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  # expect NAME WANT ARGS...: the script run in the stub exits WANT.
  expect() {
    local name="$1" want="$2"
    shift 2
    status=0
    (cd "$work" && bash scripts/release/changelog.sh "$@") > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
  }
  fragment() {
    local name="$1"
    shift
    printf '%s\n' "$@" > "$work/$FRAGMENT_DIR/$name"
  }
  unfragment() {
    local name="$1"
    rm -- "$work/$FRAGMENT_DIR/$name"
  }

  stub_git init -q -b main
  printf 'The fragment format.\n' > "$work/$FRAGMENT_DIR/README.md"

  # --check: the names and contents it accepts, and each it refuses.
  expect "a directory holding only the README" 0 --check
  fragment 3-a-change.added.md '- An entry that runs' '  over two lines.' '  - and nests a list item.'
  expect "a well-formed fragment beside the README" 0 --check
  local bad
  for bad in 3-Upper.added.md a-no-issue.added.md 3-a.unknown.md 3-a.added.txt 3-a_b.fixed.md 3-.fixed.md; do
    fragment "$bad" '- An entry.'
    expect "the misnamed fragment $bad" 1 --check
    grep -q "$bad is not named" "$work/out" || flunk "the misnamed fragment $bad failed without naming it."
    unfragment "$bad"
  done
  fragment 4-heading.fixed.md '### Fixed' '' '- An entry.'
  expect "a fragment holding a heading" 1 --check
  unfragment 4-heading.fixed.md
  fragment 4-prose.fixed.md 'A sentence, no list item.'
  expect "a fragment of prose" 1 --check
  unfragment 4-prose.fixed.md
  fragment 4-blank.fixed.md '- One entry.' '' '- Another entry.'
  expect "a fragment with a blank line inside" 1 --check
  grep -q 'line 2 is blank' "$work/out" || flunk "the blank line was not named."
  unfragment 4-blank.fixed.md
  fragment 4-flush.fixed.md '- One entry' 'continued flush left.'
  expect "a fragment continued without indentation" 1 --check
  unfragment 4-flush.fixed.md
  printf '' > "$work/$FRAGMENT_DIR/4-empty.fixed.md"
  expect "an empty fragment" 1 --check
  unfragment 4-empty.fixed.md
  mkdir "$work/$FRAGMENT_DIR/5-dir.added.md"
  expect "a directory named as a fragment" 1 --check
  rmdir "$work/$FRAGMENT_DIR/5-dir.added.md"

  # --assemble: the [Unreleased] entries an in-flight change wrote, merged with
  # the fragments, section by section and by file name in the C locale.
  cat > "$work/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

### Upgrade notes

- An upgrade note written into CHANGELOG.md.

### Added

- An entry written into CHANGELOG.md
  before the fragments.

### Fixed

- A fix written into CHANGELOG.md.

## [0.0.1] - 2026-10-01

### Added

- The first release.

[Unreleased]: https://example.org/r/compare/v0.0.1...HEAD
[0.0.1]: https://example.org/r/releases/tag/v0.0.1
EOF
  fragment 12-b.added.md '- Fragment 12-b.'
  fragment 7-y.changed.md '- Fragment 7-y.' '' ''
  fragment 5-x.security.md '- Fragment 5-x.'
  fragment 9-u.upgrade.md '- Upgrade fragment 9-u.'
  printf '%s\n' '# Security' '' '| Release | Released | Supported until |' '| --- | --- | --- |' \
    '| v0.0.1 | 2026-10-01 | 2031-10-01 |' "$SUPPORT_MARKER" '' '## Withdrawn' > "$work/SECURITY.md"
  cp "$work/SECURITY.md" "$work/security.before"
  stub_git add -A
  stub_git commit -q -m "the stub"

  # The end date: five years on, and 29 February never shortens the period.
  [[ "$(support_end 2026-10-10)" == 2031-10-10 ]] || flunk "the end date of 2026-10-10 is $(support_end 2026-10-10)."
  [[ "$(support_end 2028-02-29)" == 2033-03-01 ]] || flunk "the end date of 2028-02-29 is $(support_end 2028-02-29)."
  [[ "$(support_end 2027-01-09)" == 2032-01-09 ]] || flunk "the end date of 2027-01-09 is $(support_end 2027-01-09)."

  expect "a version that is no version" 1 --assemble 0.0 2026-10-10
  expect "a date that is no date" 1 --assemble 0.0.2 10-10-2026
  expect "a missing argument" 2 --assemble 0.0.2
  expect "an existing version" 1 --assemble 0.0.1 2026-10-10
  cp "$work/CHANGELOG.md" "$work/before"
  grep -v -F -x "$SUPPORT_MARKER" "$work/security.before" > "$work/SECURITY.md"
  expect "a SECURITY.md with no support table" 1 --assemble 0.0.2 2026-10-10
  grep -q 'no support table' "$work/out" || flunk "the missing support table was not named."
  cmp -s "$work/before" "$work/CHANGELOG.md" || flunk "a cut refused for its support table wrote CHANGELOG.md."
  awk -v marker="$SUPPORT_MARKER" '$0 == marker { print "| v0.0.2 | 2026-10-09 | 2031-10-09 |" } { print }' \
    "$work/security.before" > "$work/SECURITY.md"
  expect "a version SECURITY.md already lists" 1 --assemble 0.0.2 2026-10-10
  grep -q 'already lists' "$work/out" || flunk "the listed version was not named."
  cp "$work/security.before" "$work/SECURITY.md"
  expect "the cut" 0 --assemble 0.0.2 2026-10-10
  grep -q 'supported until 2031-10-10' "$work/out" || flunk "the cut did not print the end of the support period."

  cat > "$work/want" <<'EOF'
# Changelog

## [Unreleased]

## [0.0.2] - 2026-10-10

Supported until 2031-10-10, 5 years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)). A security fix ships in the latest release, as [SECURITY.md](https://example.org/r/blob/main/SECURITY.md#supported-versions) says.

### Upgrade notes

- An upgrade note written into CHANGELOG.md.
- Upgrade fragment 9-u.

### Added

- An entry written into CHANGELOG.md
  before the fragments.
- Fragment 12-b.
- An entry that runs
  over two lines.
  - and nests a list item.

### Changed

- Fragment 7-y.

### Fixed

- A fix written into CHANGELOG.md.

### Security

- Fragment 5-x.

## [0.0.1] - 2026-10-01

### Added

- The first release.

[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD
[0.0.2]: https://example.org/r/compare/v0.0.1...v0.0.2
[0.0.1]: https://example.org/r/releases/tag/v0.0.1
EOF
  diff -u "$work/want" "$work/CHANGELOG.md" > "$work/out" || flunk "the assembled CHANGELOG.md differs from the expected one."
  if [[ -n "$(stub_git ls-files -- "$FRAGMENT_DIR" | grep -v '/README.md$' || true)" ]]; then
    flunk "a fragment is still tracked after the cut."
  fi
  [[ -f "$work/$FRAGMENT_DIR/README.md" ]] || flunk "the cut removed the README."
  awk -v marker="$SUPPORT_MARKER" '$0 == marker { print "| v0.0.2 | 2026-10-10 | 2031-10-10 |" } { print }' \
    "$work/security.before" > "$work/want"
  diff -u "$work/want" "$work/SECURITY.md" > "$work/out" || flunk "the cut did not add the support row to SECURITY.md."
  expect "a check after the cut" 0 --check

  # A second cut with nothing to release, and an [Unreleased] the cut cannot
  # read, are refused with CHANGELOG.md untouched.
  stub_git commit -q -a -m "the cut"
  expect "a release with no entry" 1 --assemble 0.0.3 2026-10-11
  grep -q 'would be empty' "$work/out" || flunk "the empty release was not named."

  # A pre-release rehearses the lane: no support line, no SECURITY.md row.
  cp "$work/SECURITY.md" "$work/security.before"
  fragment 13-rc.added.md '- Fragment 13-rc.'
  stub_git add -A
  stub_git commit -q -m "a fragment for the rehearsal"
  expect "a pre-release cut" 0 --assemble 0.0.3-rc.1 2026-10-11
  grep -q 'gets no support period' "$work/out" || flunk "the pre-release cut did not say it has no support period."
  if awk '/^## \[0\.0\.3-rc\.1\]/ { on = 1; next } /^## \[/ { on = 0 } on && /^Supported until/ { found = 1 } END { exit !found }' "$work/CHANGELOG.md"; then
    flunk "the pre-release section carries a support period."
  fi
  cmp -s "$work/security.before" "$work/SECURITY.md" || flunk "the pre-release cut wrote SECURITY.md."
  stub_git commit -q -a -m "the rehearsal"
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '### Improved' '' '- An entry.' '' \
    '[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD' > "$work/CHANGELOG.md"
  cp "$work/CHANGELOG.md" "$work/before"
  expect "a section Keep a Changelog does not define" 1 --assemble 0.0.3 2026-10-11
  cmp -s "$work/before" "$work/CHANGELOG.md" || flunk "a refused cut wrote CHANGELOG.md."
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' 'Loose prose.' '' '### Added' '' '- An entry.' '' \
    '[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD' > "$work/CHANGELOG.md"
  expect "text outside a section heading" 1 --assemble 0.0.3 2026-10-11
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '### Added' '' '- An entry.' > "$work/CHANGELOG.md"
  expect "no [Unreleased] link reference" 1 --assemble 0.0.3 2026-10-11

  echo "changelog: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
--check)
  [[ $# -eq 1 ]] || usage
  cd "$(dirname "$0")/../.."
  check
  ;;
--assemble)
  [[ $# -eq 3 ]] || usage
  cd "$(dirname "$0")/../.."
  assemble "$2" "$3"
  ;;
*) usage ;;
esac
