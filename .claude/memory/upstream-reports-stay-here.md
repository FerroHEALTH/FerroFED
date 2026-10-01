---
name: upstream-reports-stay-here
description: "An upstream-report issue is the record and stays in this tracker; nothing is filed on an external specification tracker and no 'owner action: file upstream' issue is created; carried from FerroBRIDGE (2026-09-13)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-13 an issue asked the owner to file seven
`upstream-report` issues on external trackers. The owner: "we off course
create the upstream report issues but we will not report it upstream so these
owner action issues please never make that again ... but we keep creating
these upstream issue reports".

**Why:** the value of a report is the cited record of the defect and of the
project's own decision, which the tracker holds.

**How to apply:** keep writing `upstream-report` issues for defects in the
Federation Tier specification, its schemas, its reference implementation, or
a bound profile (the label, the citations, what FerroFED does, the resolution
an upstream would need), with no milestone ([[upstream-reports-no-milestone]]).
Never create an issue, checklist item or sentence that asks anyone to file
them externally. Prose says "recorded as an upstream-report issue". While the
specification is a release candidate these reports matter more, since the 1.0
text may resolve them ([[spec-pin-0-9-0-rc]]).
