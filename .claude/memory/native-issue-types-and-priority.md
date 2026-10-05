---
name: native-issue-types-and-priority
description: "Type, priority and effort are GitHub's native issue type and the FerroHEALTH Priority and Effort issue fields, set with scripts/gh/fields.sh; the bug, enhancement and P0 to P3 labels are retired; owner 2026-10-02, modelled on another of the owner's projects (#154)"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Since 2026-10-02 FerroFED tracks the kind and urgency of its work the way
another of the owner's projects has since 2026-09-19: the native issue type (`Bug`, `Feature`,
`Task`), the organisation's `Priority` field (`Urgent`, `High`, `Medium`,
`Low`) and its `Effort` field (`High`, `Medium`, `Low`). The FerroHEALTH
organisation carries all three.

**Why:** on 2026-10-02 the owner asked for an issue to adopt this and to
remove the P0 to P3 labels, pointing at that project's `scripts/gh/fields.sh`
and its rules and agents, and for the whole codebase and the label setup to
move with it. Labels for type and priority duplicate
what GitHub now models natively, and the board and the issue list can filter
on the fields.

**How to apply:**

- File every issue with `scripts/gh/fields.sh new <type> <priority> <effort>
  <gh issue create args…>`; set or change one with `fields.sh type|priority|
  effort <n> <value>`; read with `fields.sh show <n>`.
- A Task carries exactly one work-kind label (`documentation`, `chore`,
  `refactor`, `perf`, `test`, `ci`); a Bug or a Feature carries none.
- The migration is `scripts/gh/migrate-fields.sh plan|apply|verify`, and it
  runs before `scripts/gh/labels.sh` deletes the six old labels. The auto-mode
  classifier refused the `apply` run from an agent session on 2026-10-02, so
  the owner runs `apply`, `verify` and then `labels.sh` by hand.
- A scheduled lane's default token gets `organization: null` for the issue
  types; `fields.sh new` then files with the labels alone and whoever picks the
  issue up sets the three.
- Read an issue with `gh issue view <n> --json title,body,comments`; on gh
  2.101.0 `--comments` prints nothing for an issue without comments.
- Related: [[repo-in-ferrohealth-org]], [[pr-auto-merge]].
