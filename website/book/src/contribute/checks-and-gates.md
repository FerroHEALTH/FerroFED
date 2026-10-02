<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Checks and gates

Every check is a committed script or a pinned tool you can run yourself. The
design of the workflows is in
[`docs/ci-cd.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/ci-cd.md);
this page is the short version.

## Two required checks

A pull request merges into `main` only when two checks pass:

- **`conclusion`**, the one job of `ci.yml` that passes when every other job
  in it passed or was skipped;
- **`contribution-licence-guard`**, which reads the licence checkbox in the
  pull request body.

`main` also requires signed commits and a branch that is up to date with it.

## The two tiers of `ci.yml`

The first tier runs on every change, because it needs no Rust:

| Check | What it guards |
|---|---|
| zizmor | workflow security |
| actionlint | workflow correctness |
| shellcheck | every first-party shell script |
| hadolint | every first-party Dockerfile |
| comment-style | the comment budgets of the Rust sources |
| file-length | no hand-written Rust file over 1000 lines |
| versions | every repeated pin agrees with `docs/VERSIONS.md` |
| favicon-sync | the book's favicons match the brand mark |
| conformance-matrix | the [conformance matrix](../evaluate/conformance.md) agrees with the specification and with the tests that claim each point, and the README conformance badges agree with the matrix and the AQL golden pass list |
| crate-version-guard self-test | the crate-version guard judges only what a pull request changes, against a stub repository |

The second tier is the Rust lane: formatting, clippy, the tests, the
end-to-end suite against two containerised nodes, rustdoc, `cargo deny`, the
MSRV build, every feature of each published crate on its own, the packaging
dry run, the crate-version guard and dependency review. A `detect` job gates
it on the root `Cargo.toml`, which exists, so the tier runs on every change;
the crate-version guard and dependency review run on pull requests only.

## Running them locally

```sh
bash scripts/checks/versions.sh --self-test
bash scripts/checks/versions.sh
bash scripts/checks/comment-style.sh --all
bash scripts/checks/file-length.sh
bash scripts/checks/favicon-sync.sh
bash scripts/checks/conformance-matrix.sh
bash scripts/checks/crate-version-guard.sh --self-test
find scripts .claude/hooks -name '*.sh' -exec shellcheck --severity=style {} +
actionlint
zizmor --min-severity=low .github/
```

## Advisory analyzers

CodeQL, OpenSSF Scorecard and SonarQube Cloud run too. Their findings are
read and weighed, but they are advisory and never a reason on their own to
change code.
