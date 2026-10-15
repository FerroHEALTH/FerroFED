#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Installs the two build tools cargo-leptos runs for the operator console's
# site bundle, the Tailwind CSS standalone CLI and Binaryen's wasm-opt, each
# at its pinned release and checked against its pinned SHA-256 before it is
# kept, so cargo-leptos finds both on PATH and never downloads either itself
# (no specification governs this: our own design).
#
# cargo-leptos 0.3.11 runs a `tailwindcss` or `wasm-opt` it finds on PATH, and
# otherwise downloads the release archive and runs it unchecked. The versions
# and digests below are the rows of docs/VERSIONS.md, which
# scripts/checks/versions.sh holds them to; each digest equals the one the
# upstream release publishes beside its asset.
#
# Usage:
#   scripts/release/viewer-tools.sh DIR   install both into DIR/bin; put
#                                         DIR/bin on PATH before the build
#   scripts/release/viewer-tools.sh --check
#                                         fail unless PATH holds both at
#                                         their pinned versions
set -Eeuo pipefail

tailwind_version="4.3.3"
tailwind_linux_x64_sha256="dc61b3ac6b8c9ca874c0cc4c57b2409791a64c5540404ca5f5367360babc313a"
tailwind_macos_arm64_sha256="cdf646702987a743464dff4d9c60fd4480d1c1e73dd819a9a67f1078815dce9d"
binaryen_version="version_123"
binaryen_x86_64_linux_sha256="e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe"
binaryen_arm64_macos_sha256="74428be348c1a09863e7b642a1fa948cabf8ec9561052233d8288e941951725b"

die() {
  echo "viewer-tools: $*" >&2
  exit 1
}

# Fails unless the tools on PATH are the pinned releases.
check() {
  local said
  command -v tailwindcss > /dev/null ||
    die "no tailwindcss on PATH; run scripts/release/viewer-tools.sh DIR and put DIR/bin on PATH"
  command -v wasm-opt > /dev/null ||
    die "no wasm-opt on PATH; run scripts/release/viewer-tools.sh DIR and put DIR/bin on PATH"
  said="$(tailwindcss --help 2>&1 | head -n 1 || true)"
  case "$said" in
    *"tailwindcss v$tailwind_version") ;;
    *) die "tailwindcss on PATH says '$said', not v$tailwind_version" ;;
  esac
  said="$(wasm-opt --version 2>&1 || true)"
  case "$said" in
    *"($binaryen_version)") ;;
    *) die "wasm-opt on PATH says '$said', not $binaryen_version" ;;
  esac
}

# Checks FILE against the SHA-256 DIGEST, with the tool the platform has.
verify() {
  local file="$1" digest="$2"
  if command -v sha256sum > /dev/null; then
    printf '%s  %s\n' "$digest" "$file" | sha256sum -c --quiet -
  else
    printf '%s  %s\n' "$digest" "$file" | shasum -a 256 -c --quiet -
  fi
}

# Downloads URL into FILE.
fetch() {
  curl --fail --silent --show-error --location --retry 3 --output "$2" "$1"
}

install() {
  local dir="$1" tailwind_asset tailwind_sha binaryen_asset binaryen_sha
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      tailwind_asset="tailwindcss-linux-x64"
      tailwind_sha="$tailwind_linux_x64_sha256"
      binaryen_asset="binaryen-$binaryen_version-x86_64-linux.tar.gz"
      binaryen_sha="$binaryen_x86_64_linux_sha256"
      ;;
    Darwin-arm64)
      tailwind_asset="tailwindcss-macos-arm64"
      tailwind_sha="$tailwind_macos_arm64_sha256"
      binaryen_asset="binaryen-$binaryen_version-arm64-macos.tar.gz"
      binaryen_sha="$binaryen_arm64_macos_sha256"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m); install both tools by hand" ;;
  esac
  mkdir -p "$dir/bin" "$dir/download"
  fetch "https://github.com/tailwindlabs/tailwindcss/releases/download/v$tailwind_version/$tailwind_asset" \
    "$dir/download/$tailwind_asset"
  verify "$dir/download/$tailwind_asset" "$tailwind_sha"
  cp "$dir/download/$tailwind_asset" "$dir/bin/tailwindcss"
  chmod 755 "$dir/bin/tailwindcss"
  fetch "https://github.com/WebAssembly/binaryen/releases/download/$binaryen_version/$binaryen_asset" \
    "$dir/download/$binaryen_asset"
  verify "$dir/download/$binaryen_asset" "$binaryen_sha"
  tar -xzf "$dir/download/$binaryen_asset" -C "$dir"
  # The macOS wasm-opt loads libbinaryen from the lib directory beside its
  # own, so it stays in its tree and PATH reaches it through a link.
  ln -sf "$dir/binaryen-$binaryen_version/bin/wasm-opt" "$dir/bin/wasm-opt"
  PATH="$dir/bin:$PATH" check
  echo "viewer-tools: tailwindcss v$tailwind_version and wasm-opt $binaryen_version are in $dir/bin"
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
