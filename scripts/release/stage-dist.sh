#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Stages the published musl binaries of one release under dist/<os>_<arch>/,
# the layout docker/Dockerfile copies from, so the image can be built locally
# from the bytes the release lane attached rather than from a fresh compile.
#
# Each tarball is checked against the .sha256sum file published beside it
# before a byte is unpacked; a mismatch stops the script with nothing staged
# for that platform.
#
# Usage:
#   scripts/release/stage-dist.sh <version>      # for example 0.0.1
#   docker buildx build -f docker/Dockerfile --platform linux/arm64 \
#     -t ghcr.io/ferrohealth/ferrofed:0.0.1 --load .
#
# Needs the GitHub CLI (`gh`) and `sha256sum` or `shasum`. No specification
# governs this file; it is FerroFED's own design.

set -euo pipefail

die() {
  echo "stage-dist: $*" >&2
  exit 1
}

[[ "$#" -eq 1 ]] || die "usage: $0 <version>"
version="${1#v}"
case "$version" in
'' | *[!0-9A-Za-z.+-]*) die "'$1' is not a version" ;;
*) ;;
esac

command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is not installed"
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
cd "$root"

work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT

# The Docker platform suffix each published musl target is staged under.
targets="x86_64-unknown-linux-musl|linux_amd64
aarch64-unknown-linux-musl|linux_arm64"

while IFS='|' read -r target platform; do
  asset="ferrofed-v${version}-${target}.tar.gz"
  gh release download "v${version}" --repo FerroHEALTH/FerroFED \
    --pattern "$asset" --pattern "$asset.sha256sum" --dir "$work" --clobber ||
    die "release v${version} has no $asset"

  want="$(awk '{print $1; exit}' "$work/$asset.sha256sum")"
  got="$(sha "$work/$asset")"
  [[ -n "$want" ]] && [[ "$want" = "$got" ]] ||
    die "$asset: sha256 $got does not match the published $want"

  mkdir -p "dist/$platform"
  tar -xzf "$work/$asset" -C "dist/$platform" ferrofed
  chmod 0555 "dist/$platform/ferrofed"
  echo "staged dist/$platform/ferrofed from $asset ($got)"
done <<< "$targets"
