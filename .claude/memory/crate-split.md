---
name: crate-split
description: "Owner 2026-10-01: spec-derived and generated crates are split from the app crates, as in every Ferro product; whether the library crates are published to crates.io is not yet decided"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Asked on 2026-10-01 whether FerroFED publishes its library crates on the
sibling model, the owner answered: "not sure yet but we will definitely split
spec codegenerated crates and the app crates like what we do for all our
products".

**Decided:** the layout separates spec-derived and generated code (the types
behind the specification's two published JSON Schemas, if research chooses to
generate them) from the hand-written engine crates and from `app/*`.
`app/*` and `tools/*` are never published.

**Open:** the crates.io publish decision. Until the owner decides, design each
`crates/*` member's `pub` surface as if it may become API (deliberate
visibility, `#[non_exhaustive]` where a specification enum may grow), keep the
crates-publishing rule and the version-bump guard in place but inert, and ask
before adding a publish leg to the release lane. Before proposing a crate,
check the siblings for an existing one (`openehr-query` and `openehr-its`
already exist) and say in plain words what the crate does; carried from
FerroBRIDGE's 2026-09-05 ruling, "never more crates without a second
consumer".
