---
name: file-length-rule
description: "A hand-written Rust file is at most 1000 lines and is split into a module folder at 750; owner rule from FerroBRIDGE (2026-09-26), adopted here from the first .rs file"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-26 the owner ruled that hand-written files had grown
out of hand (one interpreter file at 4600 lines): a hand-written Rust file is
at most 1000 lines, and one over 750 is split into a module folder
(`<name>/mod.rs` plus one child per concern) before it grows. Generated files
are outside the rule. FerroBRIDGE enforces it with
`scripts/checks/file-length.sh` and a ratchet allow-list.

**Why:** a 4000-line file cannot be reviewed, and every worker that touched
one grew it further.

**How to apply:** FerroFED carries the setup from FerroBRIDGE, so it starts
with the rule and an empty allow-list: the guard joins the CI guard tier with
the workspace, and no file is ever added to the allow-list here. A split
moves code only; a defect found on the way is filed.
