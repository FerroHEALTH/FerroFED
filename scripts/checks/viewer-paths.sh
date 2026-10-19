#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Fails when the operator console's site bundle names a directory of the
# host that built it (no specification governs this: our own design).
#
# scripts/release/viewer-site.sh remaps the cargo home, the toolchain and the
# checkout to fixed prefixes for the WebAssembly build, so a bundle names
# none of them. This reads the WebAssembly and its JavaScript glue for a
# home directory (/home/, /Users/, /root/, C:\Users\), a CI runner path
# (/runner/, /github/workspace), the cargo registry (.cargo/registry) and a
# rustup toolchain (.rustup/toolchains), and prints each one it finds.
#
# Usage: scripts/checks/viewer-paths.sh [pkg-dir]
#   pkg-dir defaults to target/site/pkg.
#
# It reads only the bundle the last release build wrote: the digests in
# target/viewer-site.sha256 must match, so a stale bundle is refused.
#
# Exit 0 = no host path. Exit 1 = a host path, a bundle file missing, or a
# bundle the last release build did not write.
# Exit 2 = a usage error.
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
name="ferrofed-viewer"

case "$#" in
  0) pkg="$root/target/site/pkg" ;;
  1) pkg="$1" ;;
  *)
    echo "usage: scripts/checks/viewer-paths.sh [pkg-dir]" >&2
    exit 2
    ;;
esac

# The bundle the last release build wrote, and no other: the stamp
# scripts/release/viewer-site.sh --release leaves on success names the
# digests of the two files, so a stale, a failed or a debug build is refused.
stamp="$root/target/viewer-site.sha256"
for file in "$name.wasm" "$name.js"; do
  if [ ! -s "$pkg/$file" ]; then
    echo "viewer-paths: $pkg/$file is missing; run scripts/release/viewer-site.sh --release first." >&2
    exit 1
  fi
done
if [ ! -s "$stamp" ]; then
  echo "viewer-paths: $stamp is missing: the last site build failed or was not a release build; run scripts/release/viewer-site.sh --release first." >&2
  exit 1
fi
if command -v sha256sum > /dev/null; then sum=(sha256sum); else sum=(shasum -a 256); fi
if ! (cd "$pkg" && "${sum[@]}" -c --status "$stamp"); then
  echo "viewer-paths: $pkg is not the bundle the last release build wrote ($stamp); run scripts/release/viewer-site.sh --release first." >&2
  exit 1
fi

pattern='(/home/|/Users/|/root/|[A-Za-z]:\\Users\\|/runner/|/github/workspace|\.cargo/registry|\.rustup/toolchains)[^"[:cntrl:]]*'

found=0
for file in "$name.wasm" "$name.js"; do
  if [ ! -s "$pkg/$file" ]; then
    echo "viewer-paths: $pkg/$file is missing; run scripts/release/viewer-site.sh --release first." >&2
    exit 1
  fi
  hits="$(LC_ALL=C grep -a -o -E "$pattern" "$pkg/$file" | sort -u || true)"
  if [ -n "$hits" ]; then
    count="$(printf '%s\n' "$hits" | wc -l | tr -d '[:space:]')"
    echo "::error file=scripts/release/viewer-site.sh::pkg/$file names $count path(s) of the build host; remap them in scripts/release/viewer-site.sh." >&2
    printf '%s\n' "$hits" | head -n 20 >&2
    found=1
  fi
done
[ "$found" -eq 0 ] || exit 1
echo "viewer-paths: the bundle names no directory of the build host"
