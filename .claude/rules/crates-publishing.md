---
paths: ["crates/**", "scripts/release/**", ".github/workflows/publish-crates.yml"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Published crates discipline (crates.io)

**Nothing is published to crates.io for now, and publishing is a one-line
switch** (owner decision, 2026-10-01; `docs/architecture.md` §11, decision
A35). The split is fixed (#106, decision A34): the libraries a third party
could use live under `crates/` and carry the name of the specification they
implement, never `ferrofed-*` (`openehr-federation`, `ihe-iti`,
`nl-generic-functions`, `oauth-server-metadata`;
`.claude/memory/published-crate-naming.md`), one crate per specification
with a feature per layer or profile; FerroFED's own glue and the server binary
live under `app/`, and the tools under `tools/`.

## The switch

- The root `Cargo.toml` sets `[workspace.package] publish = false`, and every
  `crates/*` member inherits it with `publish.workspace = true`. Changing that
  one value to `true` makes the library crates publishable. It is flipped only
  by the owner.
- `app/*` and `tools/*` carry a hard `publish = false` of their own, never the
  inherited one, so the switch can never reach them.
- From v0.0.2 the whole lane exists and runs while the switch is off (#32):
  `publish-crates.yml` runs on every `v*` release tag and publishes exactly
  the members whose `cargo metadata` says publishable, which today is a
  successful no-op that says so in its job summary; the `publish-dry-run` job
  of `ci.yml` packages every library crate on every pull request, so the
  crates stay publishable; and the crate-version guard and its bump hook are
  live from the first crate.
- Flipping the switch takes the two owner steps below: the `crates-io`
  environment, and a Trusted Publisher per crate on crates.io.

Design every `pub` surface of a `crates/*` member as API from the start, so
flipping the switch needs no rework. Because a member's version is guarded
from the first crate, version hygiene is a hard rule now, not at the first
publish: published versions are immutable, and the `crate-version-guard` CI
job enforces the bump rule below. The licence of each published crate
(BUSL-1.1, or Apache-2.0 for a crate other projects should be free to use) is
an owner decision recorded per crate when the switch flips, never assumed.

## Two version lines

- **The product version** is the workspace `version` in the root `Cargo.toml`
  (the server, the tools, the release tag `vX.Y.Z`).
- **The crate line** is the `version` in each `crates/*/Cargo.toml`. It never
  adopts the product version or a specification version; it is the crates' own
  SemVer line. A name is held on crates.io by a 0.0.0 placeholder published
  before the crate has content (`openehr-federation`, `ihe-iti` and
  `nl-generic-functions`, published on 2026-10-01), and the crate's line in
  the workspace starts at 0.0.1, above the placeholder. `oauth-server-metadata`
  (#551) has no placeholder on crates.io yet (read on 2026-10-05).

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

`scripts/release/publish-crates.sh` is the one implementation:

- `select` reads the publishable set from `cargo metadata` (a member whose
  `publish` is unset or names crates-io) in dependency order, never from a
  hand-kept list. While the switch is off the set is empty.
- `package` runs `cargo package` over every `crates/*` member, which builds
  and verifies the exact tarball an upload would send and works while the
  switch is off, then `cargo publish --dry-run` over the publishable set once
  there is one. A library that depends on another library
  (`nl-generic-functions` feature `nuts-auth` on `oauth-server-metadata`)
  resolves it through a `--config patch.crates-io.<name>.path=…` patch per
  depended-on `crates/*` member, because Cargo resolves a sibling through
  its local overlay only when a target registry is known, and while the
  switch is off there is none. The dry run takes no patch, so it meets the
  registry as an upload would.
- `publish` uploads the publishable members one at a time in dependency order
  and counts "already exists" as done, so a partial run is finished by
  running it again; `verify` reads the registry back before success is
  reported. Both are successful no-ops while the switch is off.

`publish-crates.yml` is the lane. On a `v*` tag it checks that the tag names
the workspace version, selects and packages, and only then runs the `publish`
job, in the `crates-io` environment; on a manual dispatch it is a dry run
unless `publish` is set, and it publishes only from `main` or a release tag.
It authenticates with crates.io Trusted Publishing (OIDC through
`rust-lang/crates-io-auth-action`), so no long-lived crates.io token exists in
the repository, and it restores no build cache (`ci-cd.md`). Once a generator
exists, its `codegen-drift` gate runs ahead of it, so a published generated
crate never disagrees with its generator.

The `publish-dry-run` job of `ci.yml` runs `publish-crates.sh package` on every
pull request, so a crate that cannot be packaged is found before a release
reaches the registry.

## Owner steps when the switch is flipped

Three of the four names exist on crates.io (the 0.0.0 placeholders of
2026-10-01), so every later version of those can go through Trusted
Publishing with no first upload by a personal token. `oauth-server-metadata`
needs its placeholder claimed the same way before the switch flips. Two
steps, done once:

1. The `crates-io` GitHub environment, with the owner as required reviewer and
   a deployment policy that admits `main` and `v*` tags.
2. On crates.io, each crate's Settings, Trusted Publishing: one GitHub entry,
   repository owner `FerroHEALTH`, repository `FerroFED`, workflow
   `publish-crates.yml`, environment `crates-io`.

Then the owner sets `publish = true` in the root `[workspace.package]`, and the
next `v*` tag publishes every library crate at its manifest version.

## Before publishing: the C-STABLE adjudication

`reliability.md` deviates from C-STABLE while the crates are unpublished. Every
pre-1.0 dependency that appears in a published crate's public API is
adjudicated before the first publish from here, and again whenever the line
graduates past `0.x`.

## Official documentation (durable citations)

- Trusted Publishing on crates.io: <https://crates.io/docs/trusted-publishing>
- `cargo publish`: <https://doc.rust-lang.org/cargo/commands/cargo-publish.html>
