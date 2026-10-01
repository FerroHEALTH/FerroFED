---
name: pr-auto-merge
description: "Every pull request gets auto-merge armed the moment it is opened (gh pr merge <n> --auto --squash --delete-branch); clean up local branches after each merge and branch from origin/HEAD; family rule carried from FerroBRIDGE (2026-09-04, 2026-09-13)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

After opening a pull request, arm auto-merge at once:

```sh
gh pr create ... && gh pr merge <n> --auto --squash --delete-branch
```

Never leave a pull request waiting for a manual merge.

**Why:** the owner, on FerroBRIDGE, 2026-09-04: "for PR's do not forget to
trigger auto merge okay!! so when the CI is green it will be merged". The
`main` ruleset requires the `conclusion` check, so auto-merge is the hand-off.

**How to apply:**

- Until `ci.yml` reports a `conclusion` check on `main`, an armed auto-merge
  waits. Say so in the hand-off; never use `--admin` without the owner asking.
- In a worktree, `gh pr merge --auto --squash --delete-branch` can end with
  `fatal: 'main' is already used by worktree`; the merge already succeeded.
  Confirm with `gh pr view --json state,mergedAt`.
- After every merge: `git fetch --prune`, delete each local branch whose
  upstream is `[gone]` and every `worktree-agent-*` branch, keep only `main`
  and branches with an open pull request (owner, FerroBRIDGE 2026-09-13:
  "it's an very very big mess"). Push every branch the moment it has a commit.
- Branch from `origin/HEAD`, never from a local `main`:
  `git fetch origin && git checkout -b <type>/<slug> origin/HEAD`. Stage named
  paths (`git add <files>`), never `git add -A` from the root.
- Never push a follow-up commit to an armed PR ([[auto-merge-follow-ups]]).
