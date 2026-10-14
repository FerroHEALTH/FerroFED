#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Writes the two documents Regulation (EU) 2025/327 Art 30(1)(d) has an EHR
# system "accompanied by": the information sheet of Art 38 and the
# instructions for use. Each is the book page's Markdown source at the
# checked-out tree, byte for byte, under a name that carries the version:
#
#   <out-dir>/ferrofed-v<version>-information-sheet.md
#   <out-dir>/ferrofed-v<version>-instructions-for-use.md
#
# Markdown is plain text, which a screen reader reads as it is, so the
# release attaches the source and builds nothing (Art 32(2)(d) asks for
# "accessible formats"). The script refuses, with nothing written, a page
# that is missing or empty and an information sheet that does not describe
# this version ("FerroFED <version>, released on").
#
# Usage:
#   scripts/release/accompanying-documents.sh <version> <out-dir>
#   scripts/release/accompanying-documents.sh --self-test
#
# <version> is X.Y.Z or X.Y.Z-pre, with or without a leading v. Exit 1 on a
# refusal, 2 on a usage error. ACCOMPANYING_ROOT names the repository root
# (the self-test points it at a stub tree). No specification governs how a
# release attaches the documents; it is FerroFED's own design.
set -euo pipefail

readonly SHEET=website/book/src/evaluate/information-sheet.md
readonly IFU=website/book/src/operate/instructions-for-use.md

die() {
  echo "accompanying-documents: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 <version> <out-dir> | --self-test" >&2
  exit 2
}

# run VERSION OUT: checks both pages and writes the two release copies.
run() {
  local version="${1#v}" out="$2" root page flat sheet_name ifu_name
  if ! printf '%s\n' "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    echo "accompanying-documents: '$1' is not an X.Y.Z version" >&2
    usage
  fi
  root="${ACCOMPANYING_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
  for page in "$SHEET" "$IFU"; do
    [[ -f "$root/$page" ]] || die "$page is missing, so v$version has no copy of it to attach"
    grep -q '[^[:space:]]' "$root/$page" || die "$page is empty"
  done
  # The sheet's release sentence may wrap, so the page is read as one line.
  flat="$(tr '\n' ' ' < "$root/$SHEET" | tr -s ' ')"
  [[ "$flat" == *"FerroFED $version, released on "* ]] ||
    die "$SHEET does not say \"FerroFED $version, released on\", so it is not the sheet of v$version"

  sheet_name="ferrofed-v${version}-information-sheet.md"
  ifu_name="ferrofed-v${version}-instructions-for-use.md"
  mkdir -p "$out"
  cp "$root/$SHEET" "$out/$sheet_name"
  cp "$root/$IFU" "$out/$ifu_name"
  echo "wrote $out/$sheet_name and $out/$ifu_name"
}

self_test() {
  local work failed=0 n=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -r '$work'" EXIT
  # fixture SHEET-VERSION: a stub tree whose sheet describes SHEET-VERSION.
  fixture() {
    local repo="$work/repo"
    if [[ -d "$repo" ]]; then rm -r "$repo"; fi
    if [[ -d "$work/out" ]]; then rm -r "$work/out"; fi
    mkdir -p "$repo/website/book/src/evaluate" "$repo/website/book/src/operate"
    printf '# Information sheet\n\nThis sheet describes FerroFED %s,\nreleased on 2026-01-02.\n' \
      "$1" > "$repo/$SHEET"
    printf '# Instructions for use\n\nStart the gateway.\n' > "$repo/$IFU"
  }
  # attempt WANT ARGS...: runs the script on the fixture and checks the exit.
  attempt() {
    local want="$1" status=0
    shift
    n=$((n + 1))
    ACCOMPANYING_ROOT="$work/repo" "$BASH" "$0" "$@" > "$work/log" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "accompanying-documents: self-test case $n ($*) exited $status, wanted $want:" >&2
      cat "$work/log" >&2
      failed=1
    fi
  }
  # check WHAT COMMAND...: records a failure when COMMAND fails.
  check() {
    local what="$1"
    shift
    if ! "$@"; then
      echo "accompanying-documents: self-test: $what" >&2
      failed=1
    fi
  }

  fixture 1.2.3
  attempt 0 1.2.3 "$work/out"
  check 'the sheet is written under its versioned name' \
    cmp -s "$work/repo/$SHEET" "$work/out/ferrofed-v1.2.3-information-sheet.md"
  check 'the instructions are written under their versioned name' \
    cmp -s "$work/repo/$IFU" "$work/out/ferrofed-v1.2.3-instructions-for-use.md"

  fixture 1.2.3-rc.1
  attempt 0 v1.2.3-rc.1 "$work/out"
  check 'a pre-release tag is read without its v' \
    test -f "$work/out/ferrofed-v1.2.3-rc.1-information-sheet.md"

  fixture 1.2.2
  attempt 1 1.2.3 "$work/out"
  check 'a sheet of another version is refused' grep -q 'not the sheet of v1.2.3' "$work/log"
  check 'a refusal writes nothing' test ! -e "$work/out"

  fixture 1.2.30
  attempt 1 1.2.3 "$work/out"
  check 'a longer version is not this version' grep -q 'not the sheet of v1.2.3' "$work/log"

  fixture 1.2.3
  rm "$work/repo/$IFU"
  attempt 1 1.2.3 "$work/out"
  check 'missing instructions are refused' grep -q 'instructions-for-use.md is missing' "$work/log"
  check 'a missing page writes nothing' test ! -e "$work/out"

  fixture 1.2.3
  printf '\n  \n' > "$work/repo/$IFU"
  attempt 1 1.2.3 "$work/out"
  check 'empty instructions are refused' grep -q 'instructions-for-use.md is empty' "$work/log"

  fixture 1.2.3
  attempt 2 1.2 "$work/out"
  attempt 2 '1.2.3;x' "$work/out"
  attempt 2 1.2.3
  check 'a usage error writes nothing' test ! -e "$work/out"

  if [[ "$failed" -ne 0 ]]; then
    echo "accompanying-documents: self-test failed." >&2
    exit 1
  fi
  echo "accompanying-documents: self-test passed ($n cases)."
}

case "${1:-}" in
--self-test)
  [[ "$#" -eq 1 ]] || usage
  self_test
  ;;
*)
  [[ "$#" -eq 2 ]] || usage
  run "$1" "$2"
  ;;
esac
