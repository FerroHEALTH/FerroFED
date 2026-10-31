<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

## What changed and why

<!-- Describe the change and the reason for it. Keep it to what a reviewer needs. -->

Closes #NNN

## Checklist

- [ ] Spec-facing decisions cite the governing specification (the Federation Tier with AQL by section, N# requirement or CP# conformance point, openEHR ITS-REST, AQL, or the IHE profile), not memory and not another implementation.
- [ ] Shell and workflow files are clean: `shellcheck --severity=style`, `actionlint`, `zizmor --min-severity=low .github/`, `hadolint --config .hadolint.yaml docker/Dockerfile`.
- [ ] The committed guards pass: every guard tier 1 of `.github/workflows/ci.yml` runs (`scripts/checks/*.sh`), and the `--self-test` of every `scripts/gh/` helper the change touched.
- [ ] Rust gates pass: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run --workspace --locked`, `cargo test --doc --locked`, `cargo doc` with `RUSTDOCFLAGS=-D warnings`, and `cargo deny check`.
- [ ] A changelog fragment, `changelog.d/<issue>-<kebab-slug>.<section>.md`, records the change if it is user-visible (`changelog.d/README.md`); otherwise the pull request carries the `no-changelog` label.
- [ ] Docs are updated, if behaviour changed.
- [ ] No patient data or real patient identifier in a fixture, test, or example.
- [ ] Every commit is signed.
- [ ] No AI or assistant attribution anywhere in the commits or this PR.
