#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# The manifest guard: every Cargo manifest of the workspace, and of the
# out-of-workspace fuzz/ crate, parses (no specification governs this: our own
# design). It runs `cargo metadata --locked --format-version 1 --no-deps` on
# each, which reads and checks every manifest without resolving a dependency
# or compiling anything, so a manifest a merge broke (a key written twice, a
# table that no longer parses) fails here first, with Cargo's own message,
# before any Rust job starts. The lockfiles are held by every --locked job and
# by the fuzz lockfile job, which resolve.
#
# Usage:
#   scripts/checks/manifests.sh              check the workspace and fuzz/
#   scripts/checks/manifests.sh --self-test  prove a manifest with a key
#                                            written twice fails, and a sound
#                                            one passes
# Needs cargo on PATH. Exits with Cargo's own non-zero code when a manifest
# does not parse, 1 when the self-test fails, 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# check MANIFEST: cargo metadata over the manifest at MANIFEST.
check() {
  cargo metadata --locked --format-version 1 --no-deps --manifest-path "$1" > /dev/null
}

self_test() {
  local work failed=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  mkdir -p "$work/sound/src" "$work/twice/src"
  printf '[package]\nname = "sound"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\n' \
    > "$work/sound/Cargo.toml"
  printf '[package]\nname = "twice"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\nbase64 = "0.23.1"\nbase64 = "0.23.1"\n' \
    > "$work/twice/Cargo.toml"
  for crate in sound twice; do
    : > "$work/$crate/src/lib.rs"
    (cd "$work/$crate" && cargo generate-lockfile --offline > /dev/null 2>&1) || true
  done

  if check "$work/sound/Cargo.toml" > /dev/null 2>&1; then
    echo "  ok: a manifest that parses passes"
  else
    echo "  FAIL: a manifest that parses was refused" >&2
    failed=1
  fi
  if check "$work/twice/Cargo.toml" > /dev/null 2>&1; then
    echo "  FAIL: a manifest with a dependency written twice passed" >&2
    failed=1
  else
    echo "  ok: a manifest with a dependency written twice fails"
  fi
  [[ "$failed" -eq 0 ]] && echo "manifests self-test: OK"
  return "$failed"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

for manifest in Cargo.toml fuzz/Cargo.toml; do
  echo "== ${manifest}"
  check "$manifest"
done
echo "manifests: OK"
