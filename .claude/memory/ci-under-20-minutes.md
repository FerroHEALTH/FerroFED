---
name: ci-under-20-minutes
description: "Every pull-request run and merge-group run of ci.yml finishes in under 20 minutes of wall time, queue included; the merge queue proves the merged tree, so the performance gate and other long non-functional gates run on the pull request and on main, never in merge_group; owner 2026-11-09"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner set a family target on 2026-11-09: a pull-request run and a
merge-group run of `ci.yml` each finish in under 20 minutes, measured from
the run's creation to its conclusion with the runner queue counted.

The merge queue proves the merged tree, never the performance. A long
non-functional gate (the A/B performance gate, fuzz, a long corpus sweep)
runs on the pull request and on every push to `main`. In a merge group its
job keeps its name and reports success without running, with a summary that
says why (#1788). Functional gates (build, clippy, tests, guards, drift,
conformance pass lists) still run in the merge group.

**Why:** the merge queue tests one pull request after another, so a 40 to
120 minute merge-group run held every waiting pull request behind it, and
hosted runners are shared across the organisation.

**How to apply:** no gate is weakened, only the event it runs on changes.
Fewer, fatter jobs beat many tiny ones, because each job waits for a runner
of its own. A new long non-functional job takes the merge-group skip in the
change that adds it.
