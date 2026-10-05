<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Contributing to FerroFED

FerroFED is one of the [FerroHEALTH](https://ferrohealth.eu/) family. The
design of record is [`docs/architecture.md`](docs/architecture.md), the build
is tracked on the issues of this repository, and the working discipline is
[`CLAUDE.md`](CLAUDE.md). Read both before making a change.

## What helps most

A change that answers an open issue, with its acceptance criteria met and the
specification section it implements cited. A citation from the Federation Tier
specification, openEHR AQL or ITS-REST, or a bound IHE profile that shows the
gateway or the design is wrong, and first-hand experience running a federated
openEHR deployment, help as much. A change to the crate layout or another
decision of the architecture of record starts as an issue with the evidence,
before any code.

## Build and test

The shell, workflow and guard set runs on every change:

```
zizmor --min-severity=low .github/
actionlint
shellcheck --severity=style <tracked shell files>
scripts/checks/comment-style.sh --all
scripts/checks/file-length.sh
scripts/checks/versions.sh
scripts/checks/favicon-sync.sh
scripts/checks/conformance-matrix.sh
scripts/checks/obligations.sh
scripts/checks/e2e-placement.sh
scripts/checks/site-links.sh
```

These are the tier-1 guards `ci.yml` runs, with the same flags, so a local
pass means a CI pass. `hadolint` lints `docker/Dockerfile` in the same tier.
`site-links.sh` needs the docs toolchain of `docs/VERSIONS.md` and lychee on
your `PATH`.
The `conclusion` job aggregates every job and is the single required check on
`main`.

The Rust gates mirror CI:

```
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --locked --all-features
cargo test --doc --workspace --locked --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps --all-features --document-private-items
cargo deny check
```

The container end-to-end suite needs Docker and runs behind its gate, as the
`e2e (containers)` job runs it:

```
FERROFED_E2E=1 cargo nextest run --locked --workspace \
  -E 'package(ferrofed-testkit) or test(/^e2e::/)'
```

A test that checks the gate lives in the `e2e` module of its crate's test
binary (`tests/it/e2e.rs` or `tests/it/e2e/`), the one place that filter
selects; `scripts/checks/e2e-placement.sh` refuses it anywhere else.

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
- Record any user-visible change as a changelog fragment,
  `changelog.d/<issue>-<kebab-slug>.<section>.md`, in the format
  `changelog.d/README.md` describes. Do not edit `CHANGELOG.md`: the release
  cut assembles the fragments into it. A pull request with no user-visible
  effect carries the `no-changelog` label instead.
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
