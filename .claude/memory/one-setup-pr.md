---
name: one-setup-pr
description: "Owner 2026-10-01: the repository setup (Claude configuration, vendored specifications, GitHub and CI) lands as one large setup pull request, with the milestones and issues created on the tracker beside it"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Offered "issues first, then several PRs" or "one big setup PR", the owner
chose one big setup PR on 2026-10-01. The same session creates the labels,
the v0.0.1 to v0.0.9 milestones, the roadmap board and the issues.

**Why:** the owner wants the project standing at once, ready for the
specification's 1.0 release the following week.

**How to apply:** this applies to the opening setup only. The setup PR names
in its body every v0.0.1 issue it closes, one `Closes #N` per issue. After it
merges, the normal cadence resumes: one PR per issue, auto-merge armed
([[pr-auto-merge]]).
