#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Writes the conformance seed data a release attaches: the vendored demo
# data `ferrofed conformance run --seed-data` reads, packed into one JSON
# file with the Apache-2.0 licence and notice it is distributed under, and
# the SHA-256 of that file in `sha256sum` form beside it.
#
#   <out-dir>/ferrofed-conformance-seed-data.json
#   <out-dir>/ferrofed-conformance-seed-data.json.sha256sum
#
# The file holds each vendored file as text, byte for byte, under its own
# name in `files`, and names its format, so the run checks each file's
# SHA-256 against the digests it carries and refuses any other content.
#
# Usage:
#   scripts/release/seed-data.sh <out-dir>
#
# Needs `jq` and `sha256sum` or `shasum`. No specification governs this file;
# it is FerroFED's own design.

set -euo pipefail

die() {
  echo "seed-data: $*" >&2
  exit 1
}

[[ "$#" -eq 1 ]] || die "usage: $0 <out-dir>"
out="$1"

readonly NAME=ferrofed-conformance-seed-data.json
readonly FORMAT=ferrofed-conformance-seed-data/1
readonly TEMPLATE="International Patient Summary.opt"
readonly HOSPITAL=composition-12345-hospital.json
readonly CLINIC=composition-12345-clinic.json

command -v jq >/dev/null 2>&1 || die "jq is not installed"
if command -v sha256sum >/dev/null 2>&1; then
  sha() {
    local file="$1"
    sha256sum "$file" | awk '{print $1}'
  }
elif command -v shasum >/dev/null 2>&1; then
  sha() {
    local file="$1"
    shasum -a 256 "$file" | awk '{print $1}'
  }
else
  die "neither sha256sum nor shasum is installed"
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
vendored="$root/docs/specs/federation-ref"
demo="$vendored/docker/demo-data"
for file in "$demo/$TEMPLATE" "$demo/$HOSPITAL" "$demo/$CLINIC" "$vendored/LICENSE" "$vendored/NOTICE" "$vendored/PROVENANCE.md"; do
  [[ -f "$file" ]] || die "$file is missing; run scripts/vendor/federation-ref.sh"
done
commit="$(sed -nE 's/^- Pin: commit .([0-9a-f]{40}).*/\1/p' "$vendored/PROVENANCE.md")"
[[ -n "$commit" ]] || die "$vendored/PROVENANCE.md names no pinned commit"

mkdir -p "$out"
jq -n \
  --arg format "$FORMAT" \
  --arg source "https://github.com/syntaric/openehr-federation-ref at commit $commit, docker/demo-data" \
  --rawfile licence "$vendored/LICENSE" \
  --rawfile notice "$vendored/NOTICE" \
  --arg template_name "$TEMPLATE" \
  --rawfile template "$demo/$TEMPLATE" \
  --arg hospital_name "$HOSPITAL" \
  --rawfile hospital "$demo/$HOSPITAL" \
  --arg clinic_name "$CLINIC" \
  --rawfile clinic "$demo/$CLINIC" \
  '{format: $format, source: $source, licence: $licence, notice: $notice,
    files: {($template_name): $template, ($hospital_name): $hospital, ($clinic_name): $clinic}}' \
  >"$out/$NAME"
printf '%s  %s\n' "$(sha "$out/$NAME")" "$NAME" >"$out/$NAME.sha256sum"
echo "wrote $out/$NAME and $out/$NAME.sha256sum"
