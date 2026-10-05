---
name: oldest-pr-merges-first
description: "The oldest open PR merges first; a newer PR opens without auto-merge until the older ones land, so long-standing PRs stop falling behind under the strict up-to-date rule; owner 2026-10-03; since 2026-10-05 main merges through the merge queue with that rule off"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-03 `main` required a branch to be up to date before it merged, and
CI took about 20 minutes. That day newer PRs kept turning green first and merging, so
#306, #296 and #316 fell behind again and again and needed a rebase each time.
#316 grew to 41 files with conflicts. The owner: "we should put some big
pressure on the long standing PR's because the rebasing is horrendous right
now", and "#296 and #306 should be worked on or merged right away because it's
getting too far behind".

**Why:** under the strict up-to-date rule, whichever armed PR goes green first
merges and pushes every other PR behind, so the oldest ones never land.

**How to apply:**
- Merge oldest first. Only the PR at the head of the order has auto-merge
  armed; a worker opens every newer PR with auto-merge off, and the
  orchestrator arms it when the PRs before it have merged.
- A PR that falls behind or conflicts gets fixed before any new work starts.
  Its worker rebases and pushes at once, after clippy and the tests, and runs
  the remaining gates while CI runs.
- Keep a PR that touches every route (a base path, a router change) small and
  quick, or land it first: it conflicts with everything.
- Admin merge (`gh pr merge --admin`) is the owner's call. The session's
  permission settings deny it to the agent ([[pr-auto-merge]]).

**Since 2026-10-05** `main` merges through GitHub's merge queue, and its
ruleset no longer requires an up-to-date branch (the strict status-check
policy is off, read on 2026-10-05): the queue tests each pull request on top
of the ones ahead of it, so a PR falls behind only when it conflicts. The
rule above was given for the strict policy ([[merge-queue-signed]]).
