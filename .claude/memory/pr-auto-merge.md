---
name: pr-auto-merge
description: "Every pull request gets auto-merge armed the moment it is opened (gh pr merge <n> --auto, which adds it to the merge queue); clean up local branches after each merge and branch from origin/HEAD; family rule carried from FerroBRIDGE (2026-09-04, 2026-09-13)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

After opening a pull request, arm auto-merge at once:

```sh
gh pr create ... && gh pr merge <n> --auto
```

`main` merges through GitHub's merge queue (2026-10-05), which sets the
merge method itself and refuses `--squash` and `--delete-branch`. The
repository deletes a merged head branch on its own.

Never leave a pull request waiting for a manual merge.

**Why:** the owner, on FerroBRIDGE, 2026-09-04: "for PR's do not forget to
trigger auto merge okay!! so when the CI is green it will be merged". The
`main` ruleset requires the `conclusion` check, so auto-merge is the hand-off.

**How to apply:**

- Until `ci.yml` reports a `conclusion` check on `main`, an armed auto-merge
  waits. Say so in the hand-off; never use `--admin` without the owner asking.
- A merge-queue entry waits for its own checks; confirm with
  `gh pr view --json state,mergedAt` after the queue reports.
- After every merge: `git fetch --prune`, delete each local branch whose
  upstream is `[gone]` and every `worktree-agent-*` branch, keep only `main`
  and branches with an open pull request (owner, FerroBRIDGE 2026-09-13:
  "it's an very very big mess"). Push every branch the moment it has a commit.
- Branch from `origin/HEAD`, never from a local `main`:
  `git fetch origin && git checkout -b <type>/<slug> origin/HEAD`. Stage named
  paths (`git add <files>`), never `git add -A` from the root.
- Never push a follow-up commit to an armed PR ([[auto-merge-follow-ups]]).
- Arming is pre-approved: `.claude/settings.json` allows
  `gh pr merge * --auto` and `gh pr merge * --auto *` (owner, 2026-10-01,
  after a session merge was refused for want of the rule). The allow covers
  `--auto` only; a plain immediate merge is still the owner's call.
- The deny list refuses `gh pr merge` with `--admin` (an immediate merge that
  bypasses the ruleset, which the allow wildcards would otherwise match) and
  with `-R`/`--repo` (the allow covers this repository only); deny wins over
  allow. Found by the commit security review on 2026-10-01.
