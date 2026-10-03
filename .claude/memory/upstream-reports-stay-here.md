---
name: upstream-reports-stay-here
description: "Upstream reports are comments on the one standing upstream-reports issue (#212), never issues of their own; the owner reports them upstream at the end, so each is written to that upstream's contributing rules; owner 2026-09-13 (FerroBRIDGE), 2026-10-02 and 2026-10-03"
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

On FerroFED on 2026-10-02, after four separate `upstream-report` issues, the
owner: "we will have just one issue for upstream and when you find more issues
… then create another comment in that one issue so we have just one big issue
with a lot of comments with upstream reports". The four were moved into #212 as
comments and closed.

**How to apply:** record each defect in the Federation Tier specification, its
schemas, its reference implementation, or a bound profile as a new comment on
#212 (a bold one-line title, the citations, what FerroFED does, the resolution
an upstream would need). Never open a new `upstream-report` issue, and give #212
no milestone ([[upstream-reports-no-milestone]]). Never create an issue,
checklist item or sentence that asks anyone to file them externally. Prose
says "recorded as an upstream report on #212". While the
specification is a release candidate these reports matter more, since the 1.0
text may resolve them ([[spec-pin-0-9-0-rc]]).

On 2026-10-03 the owner added: the reports are reported back upstream at the
end, and "you must adhere to the contributing rules" of the upstream
(https://github.com/syntaric/openehr-federation-spec/blob/main/CONTRIBUTING.md),
"otherwise you need to rewrite all reports so they are conformant". Every
comment therefore follows `.claude/rules/issue-workflow.md` § Upstream report
format: upstream and form (issue or PR) named per item, the pinned version, §,
**N#** and **CP-#** citations with anchors, and for a PR the AsciiDoc pages,
the schema change in the same change, no schema tightened beyond the prose,
nothing inside `$defs/itsRest`, and the three `tools/` checks. A report is
corrected by editing its comment in place, keeping its ids and numbering.
