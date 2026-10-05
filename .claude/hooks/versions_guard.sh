#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/versions_guard.sh
#
# Claude Code PostToolUse hook (matcher: Write|Edit).
#
# When an edited file is the pin matrix or a file that repeats a pin (the
# product version, the licence parameters, a vendored corpus's PROVENANCE.md
# or the vendor script that writes it), run scripts/checks/versions.sh. Exit 2 feeds its findings back as a correction;
# every other path is a quiet exit 0.

set -uo pipefail

payload="$(cat)" || true

if command -v jq >/dev/null 2>&1; then
  file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || true
else
  file_path="$(printf '%s' "$payload" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi

[[ -n "${file_path:-}" ]] || exit 0

# The checkout that holds the file is judged, so an edit inside a git worktree
# is checked against that worktree's tree; CLAUDE_PROJECT_DIR names the main
# checkout even then. That tree is data only: it is accepted when it is a
# worktree of this same repository (the same common git directory), and it is
# always judged by the versions script of CLAUDE_PROJECT_DIR, never by a script
# in it. A file outside this repository's checkouts is not a pin.
trusted="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
common_dir() {
  local dir
  dir="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || return 1
  realpath "$dir"
}
trusted_common="$(common_dir "$trusted")" || exit 0
repo_root="$(git -C "$(dirname "$file_path")" rev-parse --show-toplevel 2>/dev/null)" || exit 0
tree_common="$(common_dir "$repo_root")" || exit 0
[[ "$tree_common" == "$trusted_common" ]] || exit 0
rel="${file_path#"$repo_root"/}"
base="$(basename "$file_path")"

watched=0
case "$rel" in
docs/VERSIONS.md | docs/architecture.md | LICENSE | NOTICE | Cargo.toml | rust-toolchain.toml | CITATION.cff | compose.yaml | README.md) watched=1 ;;
*) ;;
esac
case "$base" in
VERSIONS.md | architecture.md | LICENSE | NOTICE | rust-toolchain.toml | CITATION.cff | compose.yaml | README.md | Dockerfile | PROVENANCE.md) watched=1 ;;
*) ;;
esac
case "$rel" in
scripts/vendor/*.sh) watched=1 ;;
*) ;;
esac

[[ "$watched" -eq 1 ]] || exit 0
guard="$trusted/scripts/checks/versions.sh"
[[ -f "$guard" ]] || exit 0

findings="$(bash "$guard" --root "$repo_root" 2>&1)" || {
  printf '%s\n' "$findings" >&2
  exit 2
}

exit 0
