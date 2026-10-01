---
name: git-c-absolute-paths
description: "A cd inside a compound Bash command does not reliably set the working directory for git in this harness; use git -C <abs path> and absolute paths for every worktree operation; carried from FerroTERM (2026-09-24)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM on 2026-09-24 a `cd <worktree> && git checkout -B <branch>` ran
in a different worktree than the one named, and the follow-up force push
overwrote a sibling pull request's remote branch with another's content.

**Why:** the Bash tool's working directory persists and is reported one call
late, so a compound command's `cd` and the harness's idea of the cwd disagree.

**How to apply:** every git call on a worktree uses `git -C /abs/path ...`,
file edits use absolute paths, and a force push is preceded by
`git -C <path> rev-parse --abbrev-ref HEAD` in the same command. See
[[auto-merge-follow-ups]] and [[clean-up-agent-worktrees]].
