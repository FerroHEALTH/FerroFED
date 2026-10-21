#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Installs the build tool cargo-leptos runs for the operator console's site
# bundle, Binaryen's wasm-opt, at its pinned release and checked against its
# pinned SHA-256 before it is kept, so cargo-leptos finds it on PATH and never
# downloads it itself (no specification governs this: our own design). The
# console draws with ferrohealth-ui's stylesheet and builds no Tailwind
# stylesheet, so no Tailwind CSS CLI is installed (#1150).
#
# cargo-leptos 0.3.11 runs a `wasm-opt` it finds on PATH, and otherwise
# downloads the release archive and runs it unchecked. The version and digests
# below are the rows of docs/VERSIONS.md, which scripts/checks/versions.sh
# holds them to; each digest equals the one the upstream release publishes
# beside its asset.
#
# Usage:
#   scripts/release/viewer-tools.sh DIR   install it into DIR/bin; put
#                                         DIR/bin on PATH before the build
#   scripts/release/viewer-tools.sh --check
#                                         fail unless PATH holds it at its
#                                         pinned version and digests
set -Eeuo pipefail

binaryen_version="version_123"
binaryen_x86_64_linux_sha256="e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe"
binaryen_arm64_macos_sha256="74428be348c1a09863e7b642a1fa948cabf8ec9561052233d8288e941951725b"
# The files inside each Binaryen archive that --check digests: wasm-opt, and
# on macOS the libbinaryen it loads, which holds the optimizer itself.
wasm_opt_x86_64_linux_sha256="d66c6724c07334155720eb2def29c434dcaaf741ce859b4f7f389e22674f9c4a"
wasm_opt_arm64_macos_sha256="4c674f02a820509828b9271665b50cd6d06b1e6696002818cdcc23314ab949e8"
libbinaryen_arm64_macos_sha256="672e18b2ba553c7960d71ef3db7c732772be1661d31119401af0b4f48e5d5e14"

die() {
  echo "viewer-tools: $*" >&2
  exit 1
}

# Prints the SHA-256 of FILE, with the tool the platform has.
digest_of() {
  if command -v sha256sum > /dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Checks FILE against the SHA-256 DIGEST.
verify() {
  local file="$1" digest="$2" found
  found="$(digest_of "$file")"
  [ "$found" = "$digest" ] || die "$file has SHA-256 $found, the pin is $digest"
}

# Prints FILE with every symbolic link on its way resolved, so the digest is
# taken of the file a link on PATH runs.
resolve() {
  local file="$1" link
  while [ -L "$file" ]; do
    link="$(readlink "$file")"
    case "$link" in
      /*) file="$link" ;;
      *) file="$(dirname "$file")/$link" ;;
    esac
  done
  (cd -P "$(dirname "$file")" && printf '%s/%s\n' "$(pwd -P)" "$(basename "$file")")
}

# Prints the first line a tool printed, with any ANSI escape removed.
first_line() {
  NO_COLOR=1 FORCE_COLOR=0 "$@" 2>&1 | head -n 1 | sed -E $'s/\x1b\\[[0-9;]*[A-Za-z]//g' || true
}

# Fails unless the wasm-opt on PATH is the pinned release, by version and by
# the SHA-256 of the files that run.
check() {
  local said wasm_opt wasm_opt_sha libbinaryen_sha=""
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      wasm_opt_sha="$wasm_opt_x86_64_linux_sha256"
      ;;
    Darwin-arm64)
      wasm_opt_sha="$wasm_opt_arm64_macos_sha256"
      libbinaryen_sha="$libbinaryen_arm64_macos_sha256"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m), so the wasm-opt on PATH cannot be checked" ;;
  esac
  wasm_opt="$(command -v wasm-opt)" ||
    die "no wasm-opt on PATH; run scripts/release/viewer-tools.sh DIR and put DIR/bin on PATH"
  said="$(first_line wasm-opt --version)"
  case "$said" in
    *"($binaryen_version)"*) ;;
    *) die "wasm-opt on PATH says '$said', not $binaryen_version" ;;
  esac
  wasm_opt="$(resolve "$wasm_opt")"
  verify "$wasm_opt" "$wasm_opt_sha"
  if [ -n "$libbinaryen_sha" ]; then
    verify "$(dirname "$wasm_opt")/../lib/libbinaryen.dylib" "$libbinaryen_sha"
  fi
}

# Downloads URL into FILE.
fetch() {
  curl --fail --silent --show-error --location --retry 3 --output "$2" "$1"
}

install() {
  local dir="$1" binaryen_asset binaryen_sha
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      binaryen_asset="binaryen-$binaryen_version-x86_64-linux.tar.gz"
      binaryen_sha="$binaryen_x86_64_linux_sha256"
      ;;
    Darwin-arm64)
      binaryen_asset="binaryen-$binaryen_version-arm64-macos.tar.gz"
      binaryen_sha="$binaryen_arm64_macos_sha256"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m); install wasm-opt by hand" ;;
  esac
  mkdir -p "$dir/bin" "$dir/download"
  fetch "https://github.com/WebAssembly/binaryen/releases/download/$binaryen_version/$binaryen_asset" \
    "$dir/download/$binaryen_asset"
  verify "$dir/download/$binaryen_asset" "$binaryen_sha"
  tar -xzf "$dir/download/$binaryen_asset" -C "$dir"
  # The macOS wasm-opt loads libbinaryen from the lib directory beside its
  # own, so it stays in its tree and PATH reaches it through a link.
  ln -sf "$dir/binaryen-$binaryen_version/bin/wasm-opt" "$dir/bin/wasm-opt"
  PATH="$dir/bin:$PATH" check
  echo "viewer-tools: wasm-opt $binaryen_version is in $dir/bin"
}

case "${1:-}" in
  --check) check ;;
  "" | -*)
    echo "usage: scripts/release/viewer-tools.sh DIR | --check" >&2
    exit 2
    ;;
  *)
    mkdir -p "$1"
    install "$(cd "$1" && pwd)"
    ;;
esac
