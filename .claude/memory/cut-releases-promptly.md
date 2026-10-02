---
name: cut-releases-promptly
description: "Cut a release the moment its milestone's code work is done; move an owner-side or upstream-blocked straggler to the next milestone instead of holding the cut (owner, 2026-10-02)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-02 v0.0.2 and v0.0.3 had every code issue closed, and the session
held both cuts: v0.0.2 waited on #123's owner-side UI items and on #17, the
re-pin to a specification release that was not out yet, and v0.0.3 waited
behind v0.0.2. The owner: "why did you not cut releases 0.0.2 and 0.0.3,
[they] are done".

**Why:** a milestone is a delivery promise for the work this repository
controls. An owner action or an upstream release is not that work, and
holding a finished release for it delays everything behind it.

**How to apply:** when a milestone's remaining open issues are all owner-side
or blocked upstream, move them to the next milestone with a comment saying
why, and cut at once (`.claude/rules/issue-workflow.md` § Milestones =
releases allows exactly this). Releases go in order, so a held release also
holds every later one; never let that queue form. Linked:
[[milestone-autonomy]], [[release-tag-is-mine]].

The owner then ruled: "we cut right away 0.0.3, skip 0.0.2". v0.0.3 carries
both milestones in one release and no `v0.0.2` tag exists; when a cut is
late, ask whether to skip the stale version rather than cutting each in turn.
