<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Contributing to FerroFED

FerroFED is one of the [FerroHEALTH](https://ferrohealth.eu/) family and has no
code yet. The design happens on the tracker of this repository, and the
tracker is the record of it. The conventions the family shares apply from the
first commit, and the working discipline is [`CLAUDE.md`](CLAUDE.md). Read it
before making a change.

## What helps most right now

Evidence, not scaffolding. A citation from the Federation Tier specification,
openEHR AQL or ITS-REST, or a bound IHE profile that settles an open question,
or first-hand experience running a federated openEHR deployment, is worth more
than a pull request that guesses at a layout. Do not open a pull request that
scaffolds a Cargo workspace or a crate structure; that decision belongs to the
research program in the v0.0.1 milestone.

## Build and test

Today the gates are the shell, workflow and guard set, and they run on every
change:

```
zizmor --min-severity=low .github/
actionlint
shellcheck --severity=style <tracked shell files>
scripts/checks/comment-style.sh --all
scripts/checks/file-length.sh
scripts/checks/versions.sh
scripts/checks/favicon-sync.sh
```

These are the tier-1 guards `ci.yml` runs, with the same flags, so a local
pass means a CI pass. `hadolint` joins them when a first-party Dockerfile
exists. The `conclusion` job aggregates them and is the single required check
on `main`.

Once the Cargo workspace exists, the local gates mirror CI exactly:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items
cargo deny check
```

Every cargo invocation uses `--locked`, and `Cargo.lock` is committed.

## Specifications are the authority

The Federation Tier with AQL specification at its pinned version, openEHR AQL
and ITS-REST, and the IHE profiles the specification binds. Read the governing
section before implementing spec-facing behaviour, and cite it in the pull
request (section, `N#` requirement, `CP-#` conformance point). Never resolve a
specification question from memory or from another implementation's
behaviour: the reference implementation is prior art, and a running CDR is a
test deployment (`.claude/rules/spec-adherence.md`). Where the specifications
are silent, say so in the text you write: "no specification governs this: our
own design".

Never put patient data in a fixture, an issue, or a pull request. Fixtures are
synthetic.

## Pull requests

- Branch from `main` with a conventional-type name (`feat/`, `fix/`, `chore/`,
  `docs/`, `refactor/`, `perf/`, `test/`, `ci/`, `build/`, `release/`).
- Every commit is signed, and the pull request body declares `Closes #<n>` for
  the tracker issue it answers, one `Closes` keyword per issue.
- No AI or assistant attribution anywhere in the commits or the pull request.
- Every first-party file carries the SPDX header
  (`SPDX-FileCopyrightText: Vernum Projecten B.V.`,
  `SPDX-License-Identifier: BUSL-1.1`).
- Add a `CHANGELOG.md` entry under `[Unreleased]` for any user-visible change.
- Never weaken, skip, or delete a test to make a build pass.
- Every workflow `uses:` is pinned to a full commit SHA with a trailing version
  comment; `permissions:` is `{}` at the workflow level with the minimum
  granted per job, and no untrusted context is interpolated into a `run:`
  block.
- Write prose to `.claude/rules/writing-style.md`: no em dashes, no
  "not X but Y", no decorative triads, no filler buzzwords.

## Using AI tools

You may. If a contribution has AI-generated content, say so in the pull-request
description: which tool, and what it did. Disclosure lives in the description
and never in a commit trailer. You remain responsible for the submission in
full: understood, explained on request, tested, and honest. See
[`AI_STATEMENT.md`](AI_STATEMENT.md).

## Licensing of contributions

FerroFED's own code is licensed under the Business Source License 1.1
([`LICENSE`](LICENSE)). By submitting a contribution you:

1. certify that you wrote it, or otherwise have the right to submit it under
   these terms;
2. license it under the Business Source License 1.1 as applied to the version it
   lands in, including that version's Change License, so it becomes Apache 2.0
   with the rest of that version; and
3. grant the Licensor named in `LICENSE` a perpetual, irrevocable, worldwide,
   royalty-free, transferable right to use, reproduce, modify, distribute,
   sublicense and relicense the contribution as part of the Licensed Work under
   any terms, including commercial licences.

You keep your copyright. Point 3 is what lets the Licensed Work stay one work
with one licensor: a commercial licence, a change of the licence parameters, or a
transfer of the project can then cover every line, not only the maintainer's own.
There is no separate agreement to sign: the pull request template carries a
checkbox recording your acceptance of these terms, and a pull request from a
person does not merge without it (the `contribution-licence-guard` check, backed
by `scripts/checks/contribution-licence.sh`).

## Security

Report a vulnerability privately through the address in
[`SECURITY.md`](SECURITY.md), never in a public issue.
