---
name: deps-latest-sweep
description: "Every work session compares the workspace pins with their latest crates.io releases and checks whether FerroEHR split or added an openehr-* crate; the openEHR family lands together by hand; carried from FerroBRIDGE (owner 2026-09-24)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-24 the owner asked: "please also update all the
crates to latest version please and also understand if there are new crates
like openehr-sdt because it's split off the ITS crate". FerroEHR had split
`openehr-sdt` out of `openehr-its` that day.

**Why:** the `openehr-*` crates are one lockstep line the sibling reshapes
without notice here, and a Dependabot bump per crate never builds alone.

**How to apply:** at the start of a work session and before a release cut,
compare every `[workspace.dependencies]` pin with crates.io
(`max_stable_version`), list `../FerroEHR/crates/` against the
`openehr-*` set the workspace takes, and read the split or bump commit for the
module moves. Land the family together with its pin rows, the lock and
`deny.toml`. Check the vendored corpora against their upstream heads at the
same time; the Federation Tier specification moves fast while it is a release
candidate ([[spec-pin-0-9-0-rc]]).
