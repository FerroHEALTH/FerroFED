---
name: upstream-reports-no-milestone
description: "An upstream-report issue never carries a milestone, since a FerroFED release cannot resolve it; the in-repo decision it forces is a separate, milestoned issue; carried from FerroBRIDGE (2026-09-13)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-13 seven `upstream-report` issues sat in a
milestone. The owner: "all the upstream-report ones do never belong in an
milestone because they can not be solved right!!!"

**Why:** a milestone is a delivery promise this repository can keep. An
upstream report closes when the specification or library changes, on a
timeline nobody here controls.

**How to apply:** file an `upstream-report` issue with labels only, never
`--milestone`. When one is found in a milestone, take it out
(`gh issue edit <n> --milestone ""`). The FerroFED-side decision the defect
forces (the adjudication, the refusal, the workaround) is its own issue, and
that one carries the milestone.
