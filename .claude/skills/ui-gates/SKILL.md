---
name: ui-gates
description: >
  Runs the full operator console quality-gate battery for
  app/ferrofed-console: formatting, clippy, nextest, the crate-boundary
  tests, the stray-literal check and the browser journeys. Use before
  committing any console change, when the user asks to "check the UI", or as
  the done-gate a ui-implementer task must pass.
allowed-tools: Bash, Read, Grep, Glob
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /ui-gates

Run every gate the console must pass (`.claude/rules/console-ui.md` §4). Stop
and report on the first hard failure; run the cheap gates first.

## Preconditions

- Shared `./target`, no ad-hoc `RUSTFLAGS`, no flag variation between runs.
- For the journeys: Chrome for Testing and its chromedriver at the release
  CI pins, installed once with `scripts/release/journey-tools.sh
  target/journey-tools`. Ask before installing anything.

## The battery, in order

Stage 0 is a read of the diff, before any build: FerroUI leads the UI
(`.claude/rules/console-ui.md`, its opening section). A new component that
FerroUI has or should have, any stylesheet, any script, or a `style`
attribute is a FAIL whose fix is an issue filed in FerroHEALTH/FerroUI.

```bash
# 0. FerroUI ownership: list what the change adds that is a stylesheet or a
#    script, and any inline style, and read every hit against the rule above.
git diff --name-status origin/main -- app/ferrofed-console \
  | grep -E '\.(css|js|mjs)$' || echo "no stylesheet or script file"
git diff origin/main -- app/ferrofed-console/src | grep -nE '^\+.*(style=|<style|<script)' \
  || echo "no inline style or script"

# 1. Formatting.
cargo fmt --all --check

# 2. Clippy over every target.
cargo clippy --locked -p ferrofed-console --all-targets --all-features -- -D warnings

# 3. Tests: the configuration, the HTTP surface in-process under the strict
#    policy, the sign-in session, the gateway client against a stub; and the
#    crate-boundary tests: the console links no part of the gateway, no Leptos
#    crate and no WebAssembly glue.
cargo nextest run --locked -p ferrofed-console
cargo nextest run --locked -p ferrofed-engine -E 'test(/architecture/)'

# 4. Every string a reader sees is in the catalog.
bash scripts/checks/stray-literals.sh app/ferrofed-console/src
```

The browser journeys (#608) run with FerroUI's modules loaded and refused,
the 320 px reflow among them; each journey starts its own chromedriver from
`target/journey-tools`:

```sh
FERROFED_JOURNEYS=1 cargo nextest run --locked -p ferrofed-testkit \
  -E 'test(/^journeys::/)'
```

After a layout change, take the book's screenshots again and look at them
(`website/book/src/operate/operator-console.md`, The screens).

## Report

One line per gate: PASS, FAIL, or SKIPPED(reason), with the failing output
excerpted verbatim on a failure. Never mark a gate green you did not run. A
FAIL is never fixed by weakening the gate (removing a lint, deleting a test,
dropping a journey): fix the code.
