---
name: gates-on-host
description: "Every gate, tests included, runs on the host CLI, never in a pinned container (a container target volume reached 104 GB and crashed Docker on FerroTERM); carried from FerroTERM (owner 2026-09-05)"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM the owner retired the pinned-container test run on 2026-09-05:
its target volume reached 104.4 GB and crashed Docker once.

**Why:** the container existed to dodge macOS Gatekeeper, which stalls a fresh
unsigned test binary for about four minutes on first run. That cost is the
owner's to accept; a runaway cache is not.

**How to apply:** run the gates in the working copy: the tier-1 guards (`zizmor`,
`actionlint`, `shellcheck`, the `scripts/checks/*.sh` set) and the Rust
gates, `cargo fmt --all --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo nextest run --workspace
--locked --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo deny check`. If a
test run seems to hang before anything prints, it is Gatekeeper; wait it out.
Container-backed end-to-end tests (a real CDR node, an identity service) run
behind an opt-in environment gate, never as the default suite.
