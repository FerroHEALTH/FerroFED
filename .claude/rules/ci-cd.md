---
paths:
  - ".github/**"
  - "scripts/**"
  - "sonar-project.properties"
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CI/CD and supply-chain discipline

No specification governs this: our own design, grounded in the OWASP GitHub
Actions Security Cheat Sheet, SLSA v1.0, OpenSSF Scorecard, and Sigstore.
This file carries the workflows that run, the build and release lanes, and
the rules every workflow keeps.

## What runs

Thirteen workflows:

- `.github/workflows/ci.yml`: the two-tier gate. Tier 1 needs no Rust
  (zizmor, actionlint, shellcheck, hadolint, kubeconform over the example
  Kubernetes manifests, the comment-style guard, the versions guard, the
  favicon guard, the site link guard over the assembled site, the
  conformance-matrix guard, the obligations guard, the
  e2e-placement guard, the tracker-helper self-tests), plus the manifest
  guard, the one tier-1 job with a toolchain, which reads every Cargo
  manifest with `cargo metadata --no-deps` and compiles nothing; tier 2 is the
  Rust set, gated behind a `detect` job that needs the manifest guard and looks
  for the root `Cargo.toml`, so it runs on every change. The
  `conclusion` job is the single required status check on `main`. The design
  is `docs/ci-cd.md`.
- `.github/workflows/contribution-licence.yml`: `contribution-licence-guard`,
  the pull-request licence checkbox, the second required check. It passes a
  merge group without reading a body, because each pull request in it was
  checked on its own.
- `main` merges through GitHub's merge queue, so a branch is never updated by
  hand. Both required checks also run on `merge_group`, and the queue tests
  each pull request on top of the ones ahead of it. Auto-merge on a pull
  request adds it to the queue.
- `.github/workflows/scorecard.yml`: OpenSSF Scorecard, an independent score of
  the repository's security posture, published to the OpenSSF API and to code
  scanning.
- `.github/workflows/codeql.yml`: CodeQL over the `actions` language, because
  the workflows in this directory are code that holds tokens, and over Rust,
  behind a detection job that looks for the root `Cargo.toml`.
- `.github/workflows/sonar.yml`: SonarQube Cloud, the multi-language sweep
  over shell, YAML, and JSON. Advisory, gating no merge
  (`ai-code-review.md`). The instrumented coverage run and the
  `sonar.projectVersion` derivation sit behind a `hashFiles('Cargo.toml')`
  step gate, which the root `Cargo.toml` opens.
- `.github/workflows/docs.yml`: the documentation site. It builds the mdBook on
  every pull request and deploys from `main` through GitHub Pages, using the
  pinned toolchain in `.github/actions/docs-toolchain`.
- `.github/workflows/release.yml`: the release lane, dormant until a `v*` tag
  is pushed. It validates the tag, checks it against every file that declares
  the product version, takes the release notes from the matching
  `CHANGELOG.md` section, creates the release as a draft, calls the three lanes
  below, and publishes only after the expected asset set is complete. The
  library crates go to crates.io through `publish-crates.yml` on the same tag,
  never through this lane. The checklist a cut follows is `docs/release.md`.
- `.github/workflows/release-build.yml`: the reusable per-target binary lane
  `cargo auditable` build with no cache, the tarball and its checksum,
  a CycloneDX source SBOM and a syft build SBOM, three attestations, and the
  provenance envelope checked before upload.
- `.github/workflows/release-image.yml`: the reusable image lane. It
  verifies this run's musl tarballs against the build lane's signer identity,
  stages them for `docker/Dockerfile`, pushes the multi-platform image to GHCR,
  attests the index and both platform manifests as OCI referrers, and verifies
  its own output as a consumer would.
- `.github/workflows/release-viewer.yml`: the reusable operator console
  lane. It builds the musl `ferrofed-viewer` binaries with `cargo auditable`
  and the site bundle with cargo-leptos, attests each, verifies them against
  its own signer, and pushes, attests and verifies
  `ghcr.io/ferrohealth/ferrofed-viewer` as the image lane does the gateway.
  The three are reusable workflows because SLSA Build Level 3 needs the
  signing identity out of reach of caller-defined steps.
- `.github/workflows/pin-freshness.yml`: the weekly freshness read over every
  pin no Dependabot ecosystem covers, the analyzer versions in `ci.yml` and the
  documentation toolchain. It opens one issue when a pin is behind its newest
  upstream release, through `scripts/gh/fields.sh new` (its default token may
  not set the issue type and fields, and the issue then lands with its label
  alone, `issue-workflow.md` §Type, priority and labels), and fails only when a
  release could not be read.
- `.github/workflows/publish-crates.yml`: the crates.io lane behind the
  workspace `publish` switch. It runs on every `v*` tag (and on a manual
  dispatch, a dry run unless `publish` is set), reads the publishable set from
  `cargo metadata`, and is a successful no-op while the switch is off. It
  shares `scripts/release/publish-crates.sh` with the `publish-dry-run` job of
  `ci.yml`; the rules are `crates-publishing.md`.
- `.github/workflows/fuzz.yml`: the `cargo fuzz` targets over the untrusted
  inputs, time-boxed and advisory, weekly, on dispatch, and on a pull request
  that touches the code a target reads.

`.github/release.yml` is a different file from the workflow: it configures
GitHub's auto-generated release notes, which the lane never uses, because a
release ships the hand-curated changelog section or it fails.

## Workflow security (every workflow, no exceptions)

- **Every `uses:` is pinned to a full commit SHA** with a trailing `# vX.Y.Z`
  comment. Dependabot (`github-actions`) bumps them. A tag or branch ref is a
  finding.
- **`permissions: {}` at workflow level**, with the minimum granted per job.
- **`persist-credentials: false`** on every `actions/checkout` that does not
  push with git.
- **No `${{ }}` context interpolation inside `run:`:** pass context through
  `env:`. This prevents template injection.
- **A publishing lane restores no build cache.** A cache an untrusted run could
  poison must not feed a release.

**Enforcement:** `ci.yml` tier 1 runs `zizmor --min-severity=low .github/`,
`actionlint`, and `shellcheck --severity=style` on every push to `main`, pull
request, and merge-group run. Run the same three by hand before pushing a
workflow or script change, so a finding costs a local run rather than a CI
round trip. The zizmor path is the whole of `.github`, so `dependabot.yml` and
every composite action under `.github/actions/` are audited alongside the
workflows. Never narrow it back to make a finding disappear: fix the cause, or
record a `# zizmor: ignore[audit]` suppression with its reason on the line the
finding names.

## Shell scripts are analysed like code

The tooling languages here are bash and Rust (`rust-style.md` §No Python).
Every committed shell script stays clean at `shellcheck --severity=style`, its
lowest floor, so every finding gates. A finding is FIXED, or it carries a
per-line `# shellcheck disable=SCnnnn` directive with its reason on the same
line. A blanket exclusion is refused, and no `.shellcheckrc` exists, because a
file that can turn a code off tree-wide eventually does.

## Rust CI lanes (tier 2 of `ci.yml`)

The lanes, with the local commands mirroring the CI
flags verbatim: `cargo fmt --all --check`; `cargo clippy --workspace
--all-targets -- -D warnings` at default features; `cargo nextest run --workspace
--locked` plus `cargo test --doc --locked`; `cargo doc` with
`RUSTDOCFLAGS=-D warnings`; `cargo deny check` (advisories, licences, bans,
sources, which subsumes cargo-audit); MSRV via `cargo hack check
--rust-version`; every feature of each published crate alone via `cargo hack
clippy --locked --each-feature --all-targets --package openehr-federation
--package ihe-iti --package nl-generic-functions -- -D warnings`, per package
and never the workspace all-features union; the `viewer` job (`cargo clippy --locked -p ferrofed-viewer --lib --target wasm32-unknown-unknown -- -D warnings`, then `scripts/release/viewer-site.sh --release`; `leptos-ui.md`); the codegen drift gate once a generator exists (`codegen.md`); the `publish-dry-run` job (`scripts/release/publish-crates.sh package`: `cargo package` over every `crates/*` member, then `cargo publish --dry-run` over the publishable set once the switch is on); the
crate-version guard on pull requests (`scripts/checks/crate-version-guard.sh`);
`dependency-review-action` on pull requests; the `e2e (containers)` job,
which sets `FERROFED_E2E=1` and runs the container-backed tests against the
digest-pinned node images (`.claude/memory/e2e-gate.md`); the `comment-style.sh` guard
at `--all`; and the golden pass list, `conformance/aql-golden/pass-list.txt`,
held by the golden AQL test in the nextest run (a listed case that stops
passing, or an unlisted pass, fails it) and by the tier-1 `conformance-matrix`
guard with the §17 matrix and the badges rendered from both. A later
conformance job that scores more of §17 records its results in that matrix
and a committed pass list held the same way, never in a record of its own.
**Always `--locked`**, so CI fails on
lockfile drift rather than on registry drift. Commit `Cargo.lock`.

## Supply chain (the release lane)

- **A release builds in a REUSABLE workflow** (`on: workflow_call`) so the
  builder is isolated and the signing identity is unreachable from build steps.
  That isolation is what makes the provenance non-falsifiable. Do not inline
  the build and attest steps back into a normal job.
- **Every release artifact carries provenance and a signed SBOM**, signed
  keyless through Sigstore (`id-token: write`), and consumers verify with
  `gh attestation verify … --signer-workflow …`.
- **A release is assembled as a draft and published last**: create the draft,
  attach every asset, check the set is complete, then publish, so a
  half-assembled release is never visible. The fix for a bad cut is a new patch
  version, never a retag, and both halves are enforced: the immutable-releases
  setting freezes a published release's notes and assets, and the
  `release-tags` ruleset stops the tag being moved or deleted (GitHub
  documentation, immutable releases and the available rules for rulesets,
  linked below).
- **A version pin has a single source of truth**, and a committed check fails
  on cross-file drift.
- **The library crates publish to crates.io through Trusted Publishing** (OIDC,
  no long-lived token), in dependency order, from `publish-crates.yml` on a
  release tag, behind the workspace `publish` switch, with the packaging dry run
  on every pull request and the codegen drift gates ahead of it, so a published
  generated crate never disagrees with its generator.

## Never

- Never unpin a `uses:` to a tag or branch, widen a job's permissions without
  cause, interpolate context into `run:`, or restore a cache in a publishing
  lane.
- Never weaken a gate to go green (`testing.md`); fix the cause.
- **Never add AI or Claude attribution** to any commit, PR, or release text.

## Official documentation (durable citations)

- OWASP GitHub Actions Security Cheat Sheet:
  <https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html>
- SLSA v1.0: <https://slsa.dev/spec/v1.0/>
- OpenSSF Scorecard: <https://github.com/ossf/scorecard-action>
- GitHub Actions security hardening:
  <https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions>
- GitHub immutable releases:
  <https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases>
- GitHub available rules for rulesets:
  <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
