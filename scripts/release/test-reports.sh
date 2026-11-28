#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Writes the test reports a release attaches (Regulation (EU) 2024/2847
# Annex VII point 6, Regulation (EU) 2025/327 Annex III item 4): the
# artifacts of the CI run that tested the tagged commit on main, packed into
# one tarball, with the SHA-256 of that tarball in `sha256sum` form beside
# it.
#
#   <out-dir>/ferrofed-<tag>-test-reports.tar.gz
#   <out-dir>/ferrofed-<tag>-test-reports.tar.gz.sha256sum
#
# The tarball holds one directory per artifact: the JUnit reports of the
# offline and the gated test runs, the node profile findings, the
# conformance report of that run and the HL7 FHIR Validator's results over
# the documents its suites wrote. The CI run is the one the push of the
# commit to main started; the script waits for it, and refuses a run that
# did not succeed or an artifact it does not hold, so no release carries a
# partial record.
#
# Usage:
#   scripts/release/test-reports.sh <tag> <commit> <out-dir>
#   scripts/release/test-reports.sh --self-test
#
# Needs gh (with GH_TOKEN and GITHUB_REPOSITORY), tar and `sha256sum` or
# `shasum`. No specification governs the tarball's form: our own design.

set -euo pipefail

readonly ARTIFACTS=(junit-offline junit-gated node-profile conformance-report eehrxf-fhir-validation)

die() {
  echo "test-reports: $*" >&2
  exit 1
}

# sha256 FILE: the SHA-256 line of FILE in sha256sum form, run where FILE is.
sha256() {
  local file="$1"
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$file"
  else
    shasum -a 256 "$file"
  fi
}

# pack TAG DIR OUT: packs the artifact directories under DIR into the
# tarball of TAG under OUT, refusing a missing or empty artifact.
pack() {
  local tag="$1" dir="$2" out="$3" name artifact
  for artifact in "${ARTIFACTS[@]}"; do
    [[ -d "$dir/$artifact" ]] && [[ -n "$(ls -A "$dir/$artifact")" ]] ||
      die "the run holds no $artifact artifact"
  done
  mkdir -p "$out"
  name="ferrofed-${tag}-test-reports"
  tar -czf "$out/$name.tar.gz" -C "$dir" "${ARTIFACTS[@]}"
  (cd "$out" && sha256 "$name.tar.gz" > "$name.tar.gz.sha256sum")
}

# fetch TAG COMMIT OUT: waits for the CI run the push of COMMIT started,
# downloads its artifacts and packs them.
fetch() {
  local tag="$1" commit="$2" out="$3" run work artifact
  [[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "'$commit' is not a commit id"
  run="$(gh run list --repo "$GITHUB_REPOSITORY" --workflow ci.yml --commit "$commit" \
    --event push --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  [[ -n "$run" ]] || die "no CI run tested the push of $commit to main"
  gh run watch "$run" --repo "$GITHUB_REPOSITORY" --interval 60 --exit-status > /dev/null ||
    die "the CI run $run of $commit did not succeed"
  work="$(mktemp -d)"
  for artifact in "${ARTIFACTS[@]}"; do
    gh run download "$run" --repo "$GITHUB_REPOSITORY" --name "$artifact" --dir "$work/$artifact" ||
      die "the CI run $run holds no $artifact artifact"
  done
  pack "$tag" "$work" "$out"
  echo "test-reports: packed the reports of CI run $run for $tag"
}

self_test() {
  local work out artifact listing
  work="$(mktemp -d)"
  out="$work/out"
  for artifact in "${ARTIFACTS[@]}"; do
    mkdir -p "$work/run/$artifact"
    printf '<testsuites/>\n' > "$work/run/$artifact/report.xml"
  done
  pack v9.9.9 "$work/run" "$out"
  listing="$(tar -tzf "$out/ferrofed-v9.9.9-test-reports.tar.gz")"
  for artifact in "${ARTIFACTS[@]}"; do
    grep -qx "$artifact/report.xml" <<< "$listing" || die "self-test: $artifact is not packed"
  done
  (cd "$out" && if command -v sha256sum > /dev/null 2>&1; then
    sha256sum -c --quiet ferrofed-v9.9.9-test-reports.tar.gz.sha256sum
  else
    shasum -a 256 -c --quiet ferrofed-v9.9.9-test-reports.tar.gz.sha256sum
  fi) || die "self-test: the checksum does not match"
  rm -r "$work/run/conformance-report"
  if (pack v9.9.9 "$work/run" "$work/refused") > /dev/null 2>&1; then
    die "self-test: a run without its conformance report is packed"
  fi
  if (fetch v9.9.9 not-a-commit "$work/refused") > /dev/null 2>&1; then
    die "self-test: a malformed commit id is accepted"
  fi
  rm -r "$work"
  echo "test-reports: self-test OK"
}

case "${1:-}" in
  --self-test)
    self_test
    ;;
  *)
    [[ $# -eq 3 ]] || die "usage: $0 <tag> <commit> <out-dir> | --self-test"
    fetch "$1" "$2" "$3"
    ;;
esac
