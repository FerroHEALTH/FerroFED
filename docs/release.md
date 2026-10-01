<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Cutting a release

A milestone is a delivery promise, and a release is cut when its milestone
reaches zero open issues. This page is the checklist the cut follows, in order,
and the record of what a published release is and is not protected against.

No specification governs this; it is FerroFED's own design, the one the
FerroHEALTH family shares. The lane it describes is
`.github/workflows/release.yml`, and the discipline every workflow here obeys
is `.claude/rules/ci-cd.md`.

## What the lane does

`release.yml` is dormant until a `v*` tag is pushed. It does not run on a push
to `main`, on a pull request, or in a merge group. `workflow_dispatch` re-runs
it for a tag that already exists and has to be dispatched at that tag.

```text
plan ── github-release (draft) ── build-binaries ── finalize-release (publish)
```

- **plan** validates the tag shape, refuses a dispatch that is not at the tag
  it names, checks the tag against every file that declares the product
  version, and extracts the `## [X.Y.Z]` section of `CHANGELOG.md` as the
  release notes. A missing or empty section fails the release, so a cut can
  never ship with notes generated from the commit range standing in for the
  changelog.
- **github-release** creates the release as a draft carrying those notes. A
  draft is mutable and invisible to anyone browsing releases, which is the
  window the asset uploads need.
- **build-binaries** is gated on a root `Cargo.toml`, the same detection
  `ci.yml` tier 2 uses. There is no Cargo workspace yet, so it reports
  `skipped` and a release with no binaries is a clean pass. It activates by
  itself when the workspace lands (v0.0.2). The attested build, the container
  image and the SBOMs replace this job with a reusable workflow under #31; the
  crates.io leg, if the research decides to publish, is #32. Neither changes
  the trigger.
- **finalize-release** checks that the draft carries every asset this version
  promises, then publishes. Publishing last means a half-assembled release is
  never visible.

Concurrency is `cancel-in-progress: false`. A second tag push queues behind the
first, because a release cancelled part-way through publishing is worse than a
slow one.

## Before the tag

1. **The milestone is empty.** `gh issue list --milestone vX.Y.Z --state open`
   answers nothing, or the owner calls the cut and moves the stragglers to the
   next milestone.
2. **The version moves in every file the pin matrix names.** Today that is
   `CITATION.cff` and the product-version row of `docs/VERSIONS.md`; the root
   `Cargo.toml` `[workspace.package]` `version` joins them when the workspace
   lands. `scripts/checks/versions.sh` fails on any file left behind, and the
   `plan` job checks the same files against the tag.
3. **The changelog names the release.** `[Unreleased]` becomes the version and
   the date, with a fresh empty `[Unreleased]` above it and a new link
   reference. What sits under the version heading is what the release notes
   say, so read it as the release notes before you tag.
4. **The version bump lands as its own pull request** and merges like any
   other: the tier-1 gates (zizmor, actionlint, shellcheck, hadolint, comment
   style, file length, versions) and the `contribution-licence-guard` are
   green on it. The Rust lanes join them when the workspace exists
   (`docs/ci-cd.md`).

## The tag

The working session pushes the tag, every cut and every rehearsal; it is never
handed to the owner as an action:

```sh
git fetch origin
git tag -s vX.Y.Z -m "vX.Y.Z" origin/main
git push origin vX.Y.Z
```

The `release-tags` ruleset requires a signature on `refs/tags/v*`, so an
unsigned tag is refused at push time. `release.yml` takes it from there, and
the session reads the run (`gh run watch`) and fixes what fails.

## Rehearsing a change to the lane

A change to `release.yml` itself is rehearsed with a pre-release tag before the
next real cut. A tag with a suffix (`v0.0.1-rc.1`) publishes as a pre-release.

1. The changelog needs a `## [0.0.1-rc.1]` section, and `CITATION.cff` and the
   product row of `docs/VERSIONS.md` must say `0.0.1-rc.1`, because the `plan`
   job checks the tag against them. The rehearsal therefore goes through its
   own version-bump pull request, exactly like a cut.
2. Push the signed `v0.0.1-rc.1` tag as above and read the run: the draft must
   carry the section as its notes, no assets, and then publish as a
   pre-release.
3. The negative case: a tag that disagrees with the matrix (say `v0.0.1-rc.2`
   while the files still say `0.0.1-rc.1`) must fail in `plan`, before any
   release exists.
4. The rehearsal tag and its pre-release stay. Tags cannot be deleted under the
   ruleset, and the record of a rehearsal is worth keeping. The next real cut
   moves the version files on to `0.0.1`.

## After the tag

1. **Read the published release.** Its notes are the changelog section, and its
   asset list is what `finalize-release` verified.
2. **Post the board status update** with what shipped and what the next
   milestone targets (`.claude/rules/project-board.md`).

## What is immutable, and what is ours

**The tag is protected.** The `release-tags` ruleset is active on
`refs/tags/v*`. It blocks `deletion` and `non_fast_forward` updates and
requires signatures. A pushed `vX.Y.Z` cannot be moved to another commit and
cannot be deleted, so the commit a release names stays the commit it was cut
from.

**The release is frozen by the platform once published.** The repository has
GitHub's immutable-releases setting on (checked on 2026-10-01 through
`GET /repos/{owner}/{repo}/immutable-releases`), so a published release's
assets and tag cannot be changed after the fact. That is why the lane attaches
everything to a draft and publishes last.

**The no-retag rule is ours, not the platform's.** A bad cut ships forward as a
new patch version. Never move a tag, never delete a release and recreate it,
and never edit a published release's notes to fix what the changelog got wrong;
fix the changelog and cut the next version. The lane enforces the half it can:
`github-release` refuses to reopen an already-published release for the same
tag, and fails with that message instead.

## Sources

- Available rules for rulesets:
  <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- Immutable releases:
  <https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases>
- Managing releases:
  <https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository>
- GitHub Actions security hardening:
  <https://docs.github.com/en/actions/security-for-github-actions/security-hardening-for-github-actions>
