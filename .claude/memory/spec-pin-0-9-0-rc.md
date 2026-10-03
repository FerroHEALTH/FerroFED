---
name: spec-pin-0-9-0-rc
description: "The Federation Tier specification is vendored at v0.9.0 (release candidate, commit 7162d0c, 2026-09-28) with the reference implementation at 92aff3c; the 1.0 release is expected the week of 2026-10-08 and the re-pin is its own issue (#17), unscheduled until 1.0 is out; owner 2026-10-01"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-01 the owner said the Federation Tier specification "will be soon
the official federation openEHR specs" and that the release lands the next
week. Asked how to pin, the owner chose to vendor the current text now and
re-pin in a dedicated issue.

- Specification: <https://github.com/syntaric/openehr-federation-spec>,
  `antora.yml` version `0.9`, display `spec-version: 0.9.0`,
  `spec-status: Release candidate`, `spec-date: 2026-09-13`; pinned commit
  `7162d0c760d23105d62a743bf0ad1073c45fdb85` (2026-09-28). CC0 1.0.
- Reference implementation: <https://github.com/syntaric/openehr-federation-ref>,
  pinned commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5` (2026-09-28),
  "RC 0.9.0 as spec bumped as well". Apache-2.0. Vendored whole as evidence,
  never an oracle and never a source of code (owner, 2026-10-01).

**Why:** work starts now so FerroFED is ready when 1.0 publishes; a pin on
the release candidate keeps every citation checkable in the meantime.

**How to apply:** cite `§`, `N#` and `CP-#` against the pinned text. When 1.0
ships, the re-pin issue diffs the release against `7162d0c` (the spec's own
"changes" page and the `CP-#` table, which only appends), moves both pins in
`docs/VERSIONS.md`, re-runs the fetch scripts, and files or edits issues for
every requirement that moved. CP numbers are stable citations per the
specification's §17, so a renumbering is itself a finding.
