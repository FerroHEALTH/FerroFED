---
name: auto-merge-follow-ups
description: "Never push a follow-up commit to a PR that has auto-merge armed unless it is still BLOCKED; a slice of work silently missed main that way on FerroTERM (2026-09-03)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM on 2026-09-03 a second slice was pushed to a pull request after
CI went green on the first; the squash merged the first slice only, the
branch kept the second, and the next branch built on the wrong base.

**Why:** auto-merge is a race with any further push; one PR, one unit of work
is the safe shape.

**How to apply:** open a new PR for each further slice (branch from
`origin/HEAD`), or check `gh pr view N --json mergeStateStatus` is `BLOCKED`
right before pushing more. After a `git stash pop` or cherry-pick, look for
`UU` in `git status` before committing. See [[pr-auto-merge]].
