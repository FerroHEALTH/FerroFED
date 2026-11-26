#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The MyHealth@EU CI-line badge guard (#1952). How it checks is our own
# design: no specification governs it.
#
# It holds:
#
#   the search     conformance/myhealth-eu-search.tsv has one row for every
#                  DocumentReference search parameter of the pinned NCPeH
#                  API's CapabilityStatementMyHealthEu, with its expectation,
#                  `answered` or `refused`, and a test function of the tree
#                  as evidence;
#   the verdicts   conformance/myhealth-eu-validation.tsv holds guide,
#                  document and verdict rows, each guide core, laboratory,
#                  discharge or vaccination and each verdict valid or
#                  invalid; the
#                  validator lane holds the rows to its run;
#   the badges     every badge scripts/conformance/myhealth-eu.sh renders
#                  equals its file, and each CI-line badge carries the EU
#                  emblem on EU blue;
#   the words      no badge label says certified, compliant or conformant:
#                  the badges are self-tested and confer nothing.
#
# Usage:
#   scripts/checks/myhealth-eu-ci.sh
#   scripts/checks/myhealth-eu-ci.sh --self-test
# Needs awk, grep, jq and tar. Exit 1 naming each refusal; 2 on a usage
# error; 0 otherwise.
set -euo pipefail
export LC_ALL=C
# shellcheck source=scripts/checks/badge-logos/logo.sh
source "$(dirname "${BASH_SOURCE[0]}")/badge-logos/logo.sh"

readonly SEARCH=conformance/myhealth-eu-search.tsv
readonly VALIDATION=conformance/myhealth-eu-validation.tsv
readonly NCP_API=docs/specs/ehdsi/myhealth.eu.fhir.ncp-api-9.1.1-ci.tgz
readonly RENDER=scripts/conformance/myhealth-eu.sh
readonly BANNED='certified|compliant|conformant'

failed=0
fail() {
  echo "::error::myhealth-eu-ci: $*" >&2
  failed=1
}

# rows FILE: the data rows of FILE, without comments and its header.
rows() {
  awk -F'\t' '/^#/ || /^$/ { next } !header { header = 1; next } { print }' "$1"
}

# capability_parameters: every DocumentReference search parameter of the
# pinned NCPeH API's server CapabilityStatement, one per line, sorted.
capability_parameters() {
  tar -xzOf "$NCP_API" package/CapabilityStatement-CapabilityStatementMyHealthEu.json |
    jq -r '.rest[].resource[] | select(.type == "DocumentReference") | .searchParam[].name' | sort
}

# check_search: the search record against the CapabilityStatement and the tree.
check_search() {
  local parameter expectation status evidence file name
  [[ -f "$SEARCH" ]] || {
    fail "$SEARCH is missing."
    return
  }
  diff -q <(capability_parameters) <(rows "$SEARCH" | cut -f1 | sort) > /dev/null ||
    fail "$SEARCH does not hold exactly the DocumentReference search parameters of $NCP_API."
  while IFS=$'\t' read -r parameter expectation status evidence; do
    case "$status" in
    answered | refused) ;;
    *) fail "$SEARCH: $parameter has status '$status', not answered or refused." ;;
    esac
    file="${evidence%%#*}"
    name="${evidence#*#}"
    if [[ ! -f "$file" ]]; then
      fail "$SEARCH: the evidence of $parameter, $file, is no file of the tree."
    elif ! grep -Eq "fn ${name}\\(" "$file"; then
      fail "$SEARCH: the evidence of $parameter names no test function $name in $file."
    fi
    [[ -n "$expectation" ]] || fail "$SEARCH: $parameter states no expectation."
  done < <(rows "$SEARCH")
}

# check_verdicts: the verdict record's rows.
check_verdicts() {
  local guide document verdict
  [[ -f "$VALIDATION" ]] || {
    fail "$VALIDATION is missing."
    return
  }
  while IFS=$'\t' read -r guide document verdict; do
    case "$guide" in core | laboratory | discharge | vaccination) ;; *) fail "$VALIDATION: '$guide' is no MyHealth@EU guide the lane checks." ;; esac
    case "$verdict" in valid | invalid) ;; *) fail "$VALIDATION: $document has verdict '$verdict', not valid or invalid." ;; esac
    [[ "$document" == *.json ]] || fail "$VALIDATION: '$document' is no emitted document."
  done < <(rows "$VALIDATION")
}

# check_badges: every badge file against a fresh rendering, and its words.
check_badges() {
  local name file fresh
  fresh="$(mktemp)"
  for name in $(bash "$RENDER" --badge-names); do
    file="$(bash "$RENDER" --badge-file "$name")"
    if ! bash "$RENDER" --badge "$name" > "$fresh" 2> /dev/null; then
      fail "$RENDER could not render the $name badge."
    elif ! diff -q "$fresh" "$file" > /dev/null 2>&1; then
      fail "$file is stale (run $RENDER --write)."
    fi
  done
  rm -f -- "$fresh"
  check_emblem conformance/myhealth-eu/*.json
  check_words conformance/myhealth-eu-badge.json conformance/myhealth-eu/*.json conformance/badges/*.json
}

# check_emblem FILE...: every badge FILE carries the EU emblem on EU blue.
check_emblem() {
  local file
  badge_logo_check "$@" || fail "a MyHealth@EU badge carries no logo."
  for file in "$@"; do
    jq -e '.namedLogo == "europeanunion" and .labelColor == "#003399"' "$file" > /dev/null 2>&1 ||
      fail "$file carries no EU emblem on EU blue."
  done
}

# check_words FILE...: no badge label of FILE says a banned word.
check_words() {
  local file
  for file in "$@"; do
    [[ -f "$file" ]] || continue
    if jq -r '.label // ""' "$file" | grep -Eiq "$BANNED"; then
      fail "the label of $file says certified, compliant or conformant; a self-tested badge confers nothing."
    fi
  done
}

self_test() {
  local work status
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # the path is fixed now, on purpose
  trap "rm -rf -- '$work'" EXIT
  printf '{"label":"MyHealth@EU Core 1.0.1 (CI)","message":"1/1 valid"}\n' > "$work/plain.json"
  printf '{"label":"MyHealth@EU Laboratory certified","message":"1/1"}\n' > "$work/certified.json"
  printf '{"label":"MyHealth@EU HDR Conformant","message":"1/1"}\n' > "$work/conformant.json"
  status=0
  (check_words "$work/plain.json" && [[ "$failed" -eq 0 ]]) > /dev/null 2>&1 || status=1
  [[ "$status" -eq 0 ]] || {
    echo "myhealth-eu-ci: self-test failed: a plain label was refused" >&2
    exit 1
  }
  for file in certified conformant; do
    failed=0
    check_words "$work/$file.json" 2> /dev/null
    [[ "$failed" -eq 1 ]] || {
      echo "myhealth-eu-ci: self-test failed: a $file label passed" >&2
      exit 1
    }
  done
  printf '{"label":"MyHealth@EU Core 1.0.1 (CI)","message":"1/1","labelColor":"#003399","namedLogo":"europeanunion","logoColor":"#FFCC00"}\n' > "$work/emblem.json"
  failed=0
  check_emblem "$work/emblem.json" 2> /dev/null
  [[ "$failed" -eq 0 ]] || {
    echo "myhealth-eu-ci: self-test failed: a badge with the EU emblem was refused" >&2
    exit 1
  }
  check_emblem "$work/plain.json" 2> /dev/null
  [[ "$failed" -eq 1 ]] || {
    echo "myhealth-eu-ci: self-test failed: a badge without a logo passed" >&2
    exit 1
  }
  failed=0
  echo "myhealth-eu-ci: self-test OK."
}

cd "$(dirname "$0")/../.."
case "${1:-}" in
--self-test) self_test ;;
'')
  check_search
  check_verdicts
  check_badges
  [[ "$failed" -eq 0 ]] && echo "myhealth-eu-ci: the search record, the verdicts and the badges hold."
  ;;
*)
  echo "usage: $0 [--self-test]" >&2
  exit 2
  ;;
esac
exit "$failed"
