#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/rust_fmt_clippy.sh
#
# Claude Code PostToolUse hook (matcher: Write|Edit).
#
# For an edited .rs file: format it with rustfmt (never blocks, since rustfmt
# failing to parse a draft is expected). For an edited .sh file: lint it
# with `shellcheck --severity=style` when the tool is available (never
# installs it; skips silently when absent), which CAN block.
#
# Then, for every file kind CI checks (a .rs file, a script under scripts/,
# a Cargo.toml, clippy.toml, the YAML under .github, the TOML under docker/
# and the conformance/*.tsv tables), run scripts/checks/comment-style.sh on
# the one file, which CAN block (exit 2) to feed its findings back as a
# correction. The guard decides what it reads, so a .tsv, .toml or YAML file
# outside those kinds passes.
#
# This hook does NOT run clippy per-edit, by design. A per-edit `cargo clippy`
# check-builds the owning crate plus its dependency cone on every file save and
# thrashes the cargo cache; clippy is a per-phase gate the agent runs
# explicitly (`cargo clippy --workspace --all-targets`).
#
# No cargo command runs here: rustfmt formats the one edited file on its own
# (edition 2024), the comment guard and shellcheck read that one file, and a
# missing tool is skipped. Any other file, and a path that no longer exists,
# is a quiet exit 0.

set -uo pipefail

payload="$(cat)" || true

if command -v jq >/dev/null 2>&1; then
  file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || true
else
  file_path="$(printf '%s' "$payload" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi

repo_root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"

case "${file_path:-}" in
*.rs | *.sh | *.toml | *.yml | *.yaml | *.tsv) ;;
*) exit 0 ;;
esac
[[ -f "$file_path" ]] || exit 0

# The checkout that holds the file, so an edit inside a git worktree is
# checked by that worktree's own guard, against that worktree's root.
file_root="$(git -C "$(dirname "$file_path")" rev-parse --show-toplevel 2>/dev/null)" ||
  file_root="$repo_root"

case "$file_path" in
*.rs)
  if command -v rustfmt >/dev/null 2>&1; then
    rustfmt --edition 2024 "$file_path" >/dev/null 2>&1 || true
  fi
  ;;
*.sh)
  if command -v shellcheck >/dev/null 2>&1; then
    findings="$(shellcheck --severity=style "$file_path" 2>&1)" || {
      printf '%s\n' "$findings" >&2
      exit 2
    }
  fi
  ;;
*) ;;
esac

# The comment-style guard reads the file when it is one of the kinds CI
# checks and passes any other. Exit 2 feeds its findings back as a correction.
guard="$file_root/scripts/checks/comment-style.sh"
if [[ -x "$guard" ]]; then
  findings="$("$guard" --files "$file_path" 2>&1)" || {
    printf '%s\n' "$findings" >&2
    exit 2
  }
fi

exit 0
