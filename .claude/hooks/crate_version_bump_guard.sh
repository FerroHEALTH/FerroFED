#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# .claude/hooks/crate_version_bump_guard.sh
#
# Claude Code PreToolUse hook (matcher: Bash).
#
# A published crates.io version is immutable, so packaged content that changes
# without a version bump is a defect that cannot be repaired after the upload
# (.claude/rules/crates-publishing.md). CI catches it on the pull request; this
# catches it before the commit exists, which is where the fix is a one-line
# edit rather than a follow-up commit.
#
# Runs scripts/checks/crate-version-guard.sh against origin/main, the base CI
# uses: WORKTREE for a `git commit`, so the staged change is what is
# checked, and HEAD for a `git push`, where the commits already exist. Exit 2
# blocks the tool call and returns the guard's findings; every other path is a
# quiet exit 0.
#
# The tree judged is the one the command runs in, never CLAUDE_PROJECT_DIR,
# which names the main checkout even when the command commits in a git
# worktree: the `git -C <dir>` of the command, else a leading `cd <dir>`,
# else the payload's `cwd`, each taken to its `git rev-parse --show-toplevel`.

set -uo pipefail

payload="$(cat)" || true

cwd=""
if command -v jq > /dev/null 2>&1; then
  command_text="$(printf '%s' "$payload" | jq -r '.tool_input.command // empty' 2>/dev/null)" || true
  cwd="$(printf '%s' "$payload" | jq -r '.cwd // empty' 2>/dev/null)" || true
else
  command_text="$payload"
fi

[[ -n "${command_text:-}" ]] || exit 0

[[ -n "$cwd" ]] || cwd="$PWD"

dir_word="(\"[^\"]+\"|'[^']+'|[^[:space:];&|]+)"
git_word="git([[:space:]]+-C[[:space:]]+$dir_word)?[[:space:]]+"
if [[ "$command_text" =~ ${git_word}commit ]]; then
  head=WORKTREE
elif [[ "$command_text" =~ ${git_word}push ]]; then
  head=HEAD
else
  exit 0
fi

# A directory word of the command, its quotes removed.
unquote() {
  local word="$1"
  word="${word#\"}"
  word="${word%\"}"
  word="${word#\'}"
  word="${word%\'}"
  printf '%s' "$word"
}

target="$cwd"
if [[ "$command_text" =~ ^[[:space:]]*cd[[:space:]]+$dir_word ]]; then
  target="$(unquote "${BASH_REMATCH[1]}")"
  [[ "$target" == /* ]] || target="$cwd/$target"
fi
if [[ "$command_text" =~ git[[:space:]]+-C[[:space:]]+$dir_word ]]; then
  git_dir="$(unquote "${BASH_REMATCH[1]}")"
  if [[ "$git_dir" == /* ]]; then
    target="$git_dir"
  else
    target="$target/$git_dir"
  fi
fi

repo_root="$(git -C "$target" rev-parse --show-toplevel 2>/dev/null)" || exit 0
guard="$repo_root/scripts/checks/crate-version-guard.sh"
[[ -x "$guard" ]] || exit 0
[[ -d "$repo_root/crates" ]] || exit 0

cd "$repo_root" || exit 0

# The base is main's tip, as CI passes it: the guard finds the merge base for
# the changed paths itself and judges versions against the tip, so a bump that
# collides with one main already took fails here as it fails in CI. No
# origin/main (a fresh clone with no fetch) means no base to judge against.
git rev-parse --verify --quiet origin/main > /dev/null || exit 0

findings="$(bash "$guard" origin/main "$head" 2>&1)" || {
  printf 'BLOCKED: a crates/* member changed its packaged content without moving its version.\n\n%s\n\n' "$findings" >&2
  printf 'Bump that member in its own Cargo.toml, move any internal requirement in the root Cargo.toml with it, run cargo update -w, and commit the lock. The published version is immutable, so this cannot be repaired later.\n' >&2
  exit 2
}

exit 0
