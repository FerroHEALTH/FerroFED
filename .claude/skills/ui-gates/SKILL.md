---
name: ui-gates
description: >
  Runs the full operator console quality-gate battery for
  app/ferrofed-viewer: formatting, clippy on the host and on wasm32,
  nextest, the crate-boundary test, and the release site bundle. Use before
  committing any viewer change, when the user asks to "check the UI", or as
  the done-gate a ui-implementer task must pass.
allowed-tools: Bash, Read, Grep, Glob
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ui-gates

Run every gate the console must pass (`.claude/rules/leptos-ui.md` §11). Stop
and report on the first hard failure; run the cheap gates first.

## Preconditions

- Tooling presence, reported rather than silently installed:
  `rustup target list --installed | grep wasm32` (add with
  `rustup target add wasm32-unknown-unknown`), `cargo leptos --version`,
  `leptosfmt --version`. The versions CI installs are the `cargo-leptos` and
  `wasm-bindgen` rows of `docs/VERSIONS.md`; cargo-leptos fetches the
  wasm-bindgen CLI that matches `Cargo.lock` when none is on `PATH`. Ask
  before installing anything.
- Shared `./target`, no ad-hoc `RUSTFLAGS`, no flag variation between runs.

## The battery, in order

```bash
# 1. Formatting. tests/ carries no view! today, and leptosfmt covers it the
#    day it does.
cargo fmt --all --check
leptosfmt --check app/ferrofed-viewer/src app/ferrofed-viewer/tests

# 2. Clippy on both halves: the host build is the server half, the wasm32
#    lib build the browser half. The wasm32 pass catches a dependency that
#    cannot compile for the browser.
cargo clippy --locked -p ferrofed-viewer --all-targets --all-features -- -D warnings
cargo clippy --locked -p ferrofed-viewer --lib --target wasm32-unknown-unknown -- -D warnings

# 3. Tests: the configuration, the HTTP surface in-process, the sign-in
#    session, the gateway client against a stub; and the crate-boundary test
#    that the console links no part of the gateway.
cargo nextest run --locked -p ferrofed-viewer
cargo nextest run --locked -p ferrofed-engine -E 'test(/architecture/)'

# 4. The release site bundle, when the change touches the build surface
#    (Cargo.toml, the [package.metadata.leptos] table, style/, the profile,
#    the script); otherwise report it skipped with the reason.
bash scripts/release/viewer-site.sh --release

# 5. The bundle budget, over the bundle stage 4 wrote: the raw and compressed
#    sizes, and a failure when the brotli-compressed WebAssembly is over the
#    budget of .claude/rules/leptos-ui.md §12. Report the sizes it prints.
bash scripts/checks/viewer-bundle.sh

# 6. The build-host paths, over the same bundle: a failure when it names a
#    home, runner, registry or toolchain directory (leptos-ui.md §12).
bash scripts/checks/viewer-paths.sh
```

Stage 4 locally: the script freezes `Cargo.lock` and fails loud when it does
not satisfy the manifests. A local cargo-leptos older than the pinned row
still builds the bundle; say which version ran. CI's `viewer` job runs the
same script at the pinned version and gates the merge, so a local skip is not
a pass.

Browser journeys are planned with the screens (#276, #277); until a journey
script exists, report that stage as `SKIPPED(no journeys yet)`.

## Report

One line per gate: PASS, FAIL, or SKIPPED(reason), with the failing output
excerpted verbatim on a failure. Never mark a gate green you did not run. A
FAIL is never fixed by weakening the gate (removing a lint, deleting a test,
dropping the wasm32 pass): fix the code.
