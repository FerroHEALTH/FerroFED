---
name: clean-up-agent-worktrees
description: "Remove a subagent's worktree and its branches as soon as its PR merges and the agent has reported, one at a time, never by looping over .claude/worktrees; carried from FerroTERM (owner 2026-09-06, caveat 2026-09-22)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM the owner asked for this three separate times (2026-09-06).
Stale worktrees under `.claude/worktrees/` pile up, each holding a full
`target/` directory, and the branch list fills with merged names.

**How to apply:** after an agent completes and its PR is merged:

```
git worktree list
gh pr list --head <branch> --state all --json number,state
git worktree unlock <path>
git worktree remove <path>
git worktree prune
git branch -D <branch> worktree-agent-<id>
```

- Never remove a worktree whose agent is still running, and never loop over
  `.claude/worktrees/*`; a loop cannot tell a live worktree from a finished
  one, and `--force` discards uncommitted work. On FerroTERM on 2026-09-22 a
  worktree removed on the merge signal lost an edit its agent was still
  committing: remove it only after the agent's final hand-back.
- A squash-merged branch fails `git branch -d`; confirm MERGED with `gh`,
  then `-D`. Run the steps separately, not as one compound command.

`.claude/worktrees/` is ignored by `.gitignore`. See [[pr-auto-merge]] and
[[git-c-absolute-paths]].
