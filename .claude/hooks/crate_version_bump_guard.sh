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
# uses: WORKTREE for a `git commit` and for the `--continue` of a merge,
# cherry-pick, rebase or revert, which commits too, so the staged change is
# what is checked, and HEAD for a `git push`, where the commits already exist. Exit 2
# blocks the tool call and returns the guard's findings; every other path is a
# quiet exit 0.
#
# The tree judged is the one the command runs in, since CLAUDE_PROJECT_DIR
# names the main checkout even when the command commits in a git worktree:
# the `git -C <dir>` of the command, else a leading `cd <dir>`, else the
# payload's `cwd`, each taken to its `git rev-parse --show-toplevel`. Only a
# worktree of this repository is judged, and always by the guard script of
# CLAUDE_PROJECT_DIR, never by a script in the judged tree.

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
elif [[ "$command_text" =~ ${git_word}(merge|cherry-pick|rebase|revert)[[:space:]]+--continue ]]; then
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

# The trusted checkout, whose guard script runs. The judged tree is data only:
# it is accepted when it is a worktree of this same repository (the same
# common git directory), and nothing in it is executed or sourced.
trusted="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
common_dir() {
  local dir
  dir="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || return 1
  realpath "$dir"
}
trusted_common="$(common_dir "$trusted")" || exit 0
repo_root="$(git -C "$target" rev-parse --show-toplevel 2>/dev/null)" || exit 0
tree_common="$(common_dir "$repo_root")" || exit 0
[[ "$tree_common" == "$trusted_common" ]] || exit 0

guard="$trusted/scripts/checks/crate-version-guard.sh"
[[ -f "$guard" ]] || exit 0
[[ -d "$repo_root/crates" ]] || exit 0

# The base is main's tip, as CI passes it: the guard finds the merge base for
# the changed paths itself and judges versions against the tip, so a bump that
# collides with one main already took fails here as it fails in CI. No
# origin/main (a fresh clone with no fetch) means no base to judge against.
git -C "$repo_root" rev-parse --verify --quiet origin/main > /dev/null || exit 0

findings="$(bash "$guard" --root "$repo_root" origin/main "$head" 2>&1)" || {
  printf 'BLOCKED: a crates/* member changed its packaged content without moving its version.\n\n%s\n\n' "$findings" >&2
  printf 'Bump that member in its own Cargo.toml, move any internal requirement in the root Cargo.toml with it, run cargo update -w, refresh fuzz/Cargo.lock with cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null (check it with --locked), and commit both locks. A member that depends on a bumped member needs its own bump too, because the requirement in its packaged manifest moved. The published version is immutable, so this cannot be repaired later.\n' >&2
  exit 2
}

exit 0
