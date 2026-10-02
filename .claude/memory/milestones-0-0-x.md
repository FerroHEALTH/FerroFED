---
name: milestones-0-0-x
description: "Release milestones start at v0.0.1 and step by 0.0.x, never a v0.1.0 opener (family rule carried from FerroBRIDGE, owner 2026-09-04); FerroFED plans v0.0.1 to v0.0.9 from the start (owner 2026-10-01)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Milestones start at `v0.0.1` and step `v0.0.2`, `v0.0.3`, never a `v0.1.0`
opener. Carried from FerroBRIDGE, where a first `v0.1.0` milestone was
renamed at the owner's request on 2026-09-04.

**Why:** the Ferro family runs an `0.0.x` line while pre-release, and the
owner wants small, frequent cuts.

**How to apply:** on 2026-10-01 the owner chose a full roadmap from the start,
v0.0.1 to v0.0.9, as FerroBRIDGE has it. v0.0.1 is the repository setup and
the research program; v0.0.2 the Cargo workspace and the first federated
query; the later milestones are thinner and gain detail as each one nears.
v0.0.1 was released on 2026-10-01; v0.0.2 and v0.0.3 shipped together as
release 0.0.3 on 2026-10-02, with no final v0.0.2 tag.
A plan laid out deliberately places each issue in the milestone its build
order implies; work found while implementing goes in the current milestone
(`.claude/rules/issue-workflow.md`). Milestones have no due dates until the
owner sets them. An `upstream-report` issue never carries one
([[upstream-reports-no-milestone]]).
