---
name: crate-split
description: "Owner 2026-10-01: spec-derived crates are split from the app crates, as in every Ferro product; nothing is published to crates.io for now, and publishing is a one-line switch (publish = false inherited from the workspace) with the whole lane built"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Asked on 2026-10-01 whether FerroFED publishes its library crates on the
sibling model, the owner answered: "not sure yet but we will definitely split
spec codegenerated crates and the app crates like what we do for all our
products".

**Decided:** the layout separates the specification crates under `crates/`
(`openehr-federation` with the hand-written wire types behind the two
published JSON Schemas, no FerroFED generator by decision A33; `ihe-iti`;
`nl-generic-functions`; `oauth-server-metadata`, split out by #551) from
FerroFED's own glue under `app/`; the crate map is `docs/architecture.md`
§11, and the naming is [[published-crate-naming]] (#106). `app/*` and `tools/*` are never published.

**Publishing, decided on 2026-10-01.** The owner: "first we will not push to
crates.io but please setup all the CI and everything so when we want to do it
we just change from publish from false to true right?". Nothing is published
for now, and the lane is built so that publishing is a one-line switch: the
root `Cargo.toml` sets `[workspace.package] publish = false`, every
`crates/*` member inherits it with `publish.workspace = true`, and `app/*` and
`tools/*` keep a hard `publish = false` of their own. From v0.0.2,
`publish-crates.yml` (#32) runs on every release tag and publishes exactly the
members whose cargo metadata says publishable (today a successful no-op), the
`publish-dry-run` job packages every library crate on every pull request,
and the crate-version guard and its bump hook are live from the first crate. Flipping the switch
takes two owner steps: the `crates-io` environment and a Trusted Publisher per
crate (`.claude/rules/crates-publishing.md`, `docs/architecture.md` §11).

**How to apply:** design each `crates/*` member's `pub` surface as API from
the start (deliberate visibility, `#[non_exhaustive]` where a specification
enum may grow), and never flip `publish` without the owner. Before proposing a crate,
check the siblings for an existing one (`openehr-query` and `openehr-its`
already exist) and say in plain words what the crate does; carried from
FerroBRIDGE's 2026-09-05 ruling, "never more crates without a second
consumer".
