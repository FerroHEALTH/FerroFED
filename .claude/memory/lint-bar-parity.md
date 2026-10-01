---
name: lint-bar-parity
description: "The family lint bar is FerroEHR's: clippy all and pedantic at deny, the strict restriction set, unwrap/expect/panic denied in application code and allowed in tests only; owner rule carried from FerroTERM (2026-09-04)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM on 2026-09-04 the owner asked "do we allow unwrap in this repo??
our FerroEHR repo is very very strict ... we also need a very very strict code
style", and on finding the table looser, "update that immediately".

**How to apply:** when the workspace lands (v0.0.2), `[workspace.lints]`
mirrors FerroEHR's and FerroBRIDGE's root `Cargo.toml`: `clippy::all` and
`pedantic` at `deny`, `as_conversions`, `pub_use`, `dead_code`,
`missing_assert_message`, `map_err_ignore`, `unused_qualifications`, the
feature-name lints, `non_ascii_idents = forbid`, `unsafe_code = forbid`, and
the rest. `clippy.toml` already relaxes the panicking lints inside tests
only. When a lint fights a legitimate case, use a scoped
`#[expect(lint, reason = "...")]`. When FerroEHR's table changes, port the
change here in the same week. See [[gates-on-host]].
