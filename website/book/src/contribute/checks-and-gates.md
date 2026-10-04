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

The first tier runs on every change, because it needs no Rust build (the
manifest check installs the toolchain only to read the manifests):

| Check | What it guards |
|---|---|
| zizmor | workflow security |
| actionlint | workflow correctness |
| shellcheck | every first-party shell script |
| hadolint | every first-party Dockerfile |
| kubeconform | the example Kubernetes manifests under `deploy/kubernetes/`, in strict mode |
| comment-style | the comment budgets of the Rust sources |
| file-length | no hand-written Rust file or book page over 1000 lines |
| versions | every repeated pin agrees with `docs/VERSIONS.md` |
| favicon-sync | the book's favicons match the brand mark |
| conformance-matrix | the [conformance matrix](../evaluate/conformance.md) agrees with the specification and with the tests that claim each point, and the README conformance badges agree with the matrix and the AQL golden pass list |
| e2e-placement | every test that checks the `FERROFED_E2E` gate lives in its crate's `e2e` test module, and the CI end-to-end job still sets the gate and selects `test(/^e2e::/)` |
| obligations | the [obligations checklist](../evaluate/obligations.md) agrees with the vendored specification, the conformance matrix and the tests it names, and a re-pin fails until every changed page is reclassified |
| site-links | every internal link and anchor of the assembled site (the landing page and this book) and of the README, checked offline by lychee, so a page or an anchor that does not exist fails the change |
| tracker-helpers | the self-tests of the `scripts/gh` tracker helpers |
| crate-version-guard self-test | the crate-version guard judges only what a pull request changes, against a stub repository |
| manifests | every Cargo manifest, of the workspace and of `fuzz/`, parses, read by `cargo metadata` without compiling, so a manifest a merge broke fails here first; the one first-tier check that installs the pinned toolchain |

The second tier is the Rust lane: formatting, the fuzz crate's lockfile,
clippy, the tests, the end-to-end suite against two containerised nodes, the
release compose files (no compose file builds the image, the release
`compose.yaml` renders, and its example `ferrofed.toml` and `registry.toml`
pass `ferrofed config check`, and so does the example Kubernetes
ConfigMap with synthetic secrets), rustdoc, `cargo deny`, the MSRV build, every feature of each published crate
on its own, the packaging dry run, the crate-version guard and dependency
review. A `detect` job gates
it on the root `Cargo.toml`, which exists, so the tier runs on every change;
the crate-version guard and dependency review run on pull requests only.

The `Docs` workflow builds this book and the landing page on every pull
request, and publishes them from `main`. A book that does not build fails it.
The required link check is the `site-links` job of `ci.yml`, because the
`Docs` build is no required check.

## Running them locally

```sh
bash scripts/checks/versions.sh --self-test
bash scripts/checks/versions.sh
bash scripts/checks/comment-style.sh --all
bash scripts/checks/file-length.sh --self-test
bash scripts/checks/file-length.sh
bash scripts/checks/favicon-sync.sh
bash scripts/checks/conformance-matrix.sh
bash scripts/checks/e2e-placement.sh --self-test
bash scripts/checks/e2e-placement.sh
bash scripts/checks/obligations.sh --self-test
bash scripts/checks/obligations.sh
bash scripts/checks/site-links.sh --self-test
bash scripts/checks/site-links.sh
bash scripts/checks/crate-version-guard.sh --self-test
bash scripts/checks/manifests.sh --self-test
bash scripts/checks/manifests.sh
for helper in fields labels migrate-fields rel; do bash "scripts/gh/$helper.sh" --self-test; done
find scripts .claude/hooks -name '*.sh' -exec shellcheck --severity=style {} +
actionlint
zizmor --min-severity=low .github/
scripts/site/assemble.sh _site
```

## Advisory analyzers

CodeQL, OpenSSF Scorecard and SonarQube Cloud run too. Their findings are
read and weighed, but they are advisory and never a reason on their own to
change code.
