---
name: milestone-autonomy
description: "Work a milestone's issues end to end without pausing to ask the order, cut the release the moment it empties, file new work as issues; family directive carried from FerroTERM (2026-09-04)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Carried from FerroTERM, where the owner said it for v0.0.4 through v0.0.7 and
restated it on 2026-09-04: "keep going till there is nothing left in the
milestone like the last one so we can cut a new release".

**Why:** deciding the order of issues inside a milestone is not something the
owner wants to be asked about.

**How to apply:** pick by priority and blockers, state the order once, then
execute. One PR per issue with auto-merge armed ([[pr-auto-merge]]), each
issue ticked and commented before moving on. New work found en route becomes
its own issue in the same or the next milestone, never a silent deferral. Cut
the release when the milestone empties, tag included ([[release-tag-is-mine]]).
This does not override the research-first rule: a foundational decision is
still put to the owner ([[owner-work-style]]), and a pause instruction still
holds.
