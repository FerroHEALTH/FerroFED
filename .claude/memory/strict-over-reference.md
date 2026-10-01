---
name: strict-over-reference
description: "The reference implementation is not the holy grail: FerroFED is as strict as FerroEHR, and where the Java reference implementation is laxer than the specification, the specification and the NEVER LAX rule win (owner, 2026-10-01)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner, 2026-10-01: "the reference implementation is not the holy grail
right? because FerroEHR is very very strict by design so we should also be
very very strict because the FED ref implementation is again built on Java".

**Why:** the reference implementation validated the 0.9.0 text, which makes it
useful evidence, but it is one implementation with its own shortcuts. The
first research pass (#20, #21, #23) already found it comparing dates and data
values as strings, deduplicating on `object_id` alone, doing storage work on
the clinical path, and claiming CP-17 with no inbound authentication. FerroEHR
is strict by design, and the gateway in front of it must not be the loose
link.

**How to apply:** `.claude/rules/spec-adherence.md` (NEVER LAX, and "its
acceptance proves nothing"). The oracle order puts the specification first;
the reference implementation is evidence, its golden cases are adjudicated
against the text, and every divergence where it is laxer is recorded (the
upstream-report drafts on #17 and the research reports). A research option
that matches the reference implementation needs its own ground, never "the
reference does it". Linked: [[owner-work-style]], [[openehr-crates-are-the-model]].
