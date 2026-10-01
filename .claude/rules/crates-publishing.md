---
paths: ["crates/**", "scripts/release/**", ".github/workflows/publish-crates.yml"]
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Published crates discipline (crates.io)

**Whether the `crates/*` members are published on crates.io is open.** The
owner's decision (2026-10-01) fixes the split and leaves the publish call to
the research program that produces `docs/architecture.md`: the specification
crates (the types the federation schemas define, generated or not,
`codegen.md`) and the gateway's library crates live under `crates/`, the
server binary under `app/`, and the tools under `tools/`. `app/*` and
`tools/*` are never published. Until the decision is recorded on the
research issue and in `docs/architecture.md`, design every `pub` surface of a
`crates/*` member as if it will be published, and publish nothing.

When the decision is to publish, the rest of this file applies as written:
published versions are immutable, so version hygiene is a hard rule,
machine-enforced by the `crate-version-guard` CI job. The licence of each
published crate (BUSL-1.1, or Apache-2.0 for a generated crate other projects
should be free to use) is an owner decision recorded per crate, never assumed.

## Two version lines

- **The product version** is the workspace `version` in the root `Cargo.toml`
  (the server, the tools, the release tag `vX.Y.Z`).
- **The crate line** is the `version` in each `crates/*/Cargo.toml`. It never
  adopts the product version or a specification version; it is the crates' own
  SemVer line. A member holds its name at the 0.0.0 placeholder until its
  first publish, and joins the line then.

## The bump rule

- **A PR that changes any packaged content of a `crates/*` member bumps THAT
  member's version in the same PR.** Packaged content is what the crate's
  `include` ships: `src/**`, `README.md`, `LICENSE`, and `Cargo.toml`. Tests,
  benches and `CLAUDE.md` are not packaged and need no bump. A root
  `[workspace.dependencies]` entry a member consumes is packaged content too,
  because `cargo package` renders the concrete requirement.
- The member's own `Cargo.toml`, any internal requirement in the root
  `[workspace.dependencies]` table, and `Cargo.lock` move together
  (`cargo update -w` in the same PR). The guard fails a half-done bump and a
  stale lock.
- Escape: the `no-crate-bump` PR label, only when the diff provably alters no
  packaged bytes.
- Not every bumped version is published; gaps in the published sequence are
  normal. Publishing different content under an existing version is what is
  forbidden, and crates.io refuses it.
- A generated crate's `src/**` changes only through its generator, so a bump
  there follows a generator change and a regeneration, never a hand-edit
  (`codegen.md`).

## The publish lane is per crate, resumable, and verified

`scripts/release/publish-crates.sh` (`publish` / `verify` / `version`) uploads
the members one at a time in dependency order, counts "already exists" as done,
and reads the registry back before reporting success. Two lanes call it: the
`crates` leg of `release.yml` on a `v*` tag (the primary path, paused by the
`crates-io` environment's required reviewer) and `publish-crates.yml` on a
manual dispatch (a dry run by default; `publish = true` is the recovery path).
Both authenticate with crates.io Trusted Publishing (OIDC through
`rust-lang/crates-io-auth-action`); no long-lived crates.io token exists in the
repository. Neither lane restores a build cache (`ci-cd.md`), and once a
generator exists its `codegen-drift` gate runs ahead of both, so a published
generated crate never disagrees with its generator.

`cargo publish --workspace --dry-run --locked` runs on every pull request as
the `publish-dry-run` job, so a packaging failure is found before a release
reaches the registry.

## Owner steps (once per crate, and once for the environment)

1. The first-ever version of a crate cannot use Trusted Publishing: the owner
   runs `cargo login` locally and `scripts/release/publish-crates.sh publish`
   from the merged `main`, then
   `scripts/release/publish-crates.sh verify`.
2. On crates.io, each crate's Settings, Trusted Publishing: two GitHub entries,
   repository owner `rubentalstra`, repository `FerroFED`, workflow
   `release.yml` and workflow `publish-crates.yml`, environment `crates-io`.
3. The `crates-io` GitHub environment carries the owner as required reviewer.

## Before publishing: the C-STABLE adjudication

`reliability.md` deviates from C-STABLE while the crates are unpublished. Every
pre-1.0 dependency that appears in a published crate's public API is
adjudicated before the first publish from here, and again whenever the line
graduates past `0.x`.

## Official documentation (durable citations)

- Trusted Publishing on crates.io: <https://crates.io/docs/trusted-publishing>
- `cargo publish`: <https://doc.rust-lang.org/cargo/commands/cargo-publish.html>
