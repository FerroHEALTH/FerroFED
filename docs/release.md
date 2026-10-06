<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
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
plan ── github-release (draft) ── build-binaries ── build-image ── finalize-release (publish)
                               └─ build-viewer ───────────────┘
```

- **plan** validates the tag shape, refuses a dispatch that is not at the tag
  it names, checks the tag against every file that declares the product
  version, and extracts the `## [X.Y.Z]` section of `CHANGELOG.md` as the
  release notes. A tag whose tree has no root `Cargo.toml` fails here, since
  the tag cannot be checked against the workspace version. A missing or empty
  changelog section fails the release, so a cut can never ship with notes
  generated from the commit range standing in for the changelog.
- **github-release** creates the release as a draft carrying those notes, and
  attaches `deploy/compose/compose.yaml`, `ferrofed.toml` and `registry.toml`
  under those names: the gateway alone at this release's image, with the two
  example files it mounts. It also attaches the conformance seed data
  `scripts/release/seed-data.sh` writes, `ferrofed-conformance-seed-data.json`
  (the vendored demo data `ferrofed conformance run --seed-data` reads, with
  its Apache-2.0 licence and notice) and its SHA-256 as
  `ferrofed-conformance-seed-data.json.sha256sum`, and the Grafana dashboard
  and Prometheus alert rules of `deploy/observability/`,
  `ferrofed-dashboard.json` and `ferrofed-alerts.yaml`. A draft is mutable and
  invisible to anyone browsing releases, which is the window the asset
  uploads need.
- **build-binaries** calls `release-build.yml` once per target (two Linux
  architectures, glibc and musl). See § The build legs.
- **build-image** calls `release-image.yml`, which builds the container from
  the attested musl binaries and pushes it to `ghcr.io/ferrohealth/ferrofed`.
- **build-viewer** calls `release-viewer.yml`, which builds the operator
  console's binaries and site bundle, packs them into a container and pushes
  it to `ghcr.io/ferrohealth/ferrofed-viewer`. The console ships as an image
  alone, so it attaches no asset to the draft.
- **finalize-release** checks that the draft carries every asset this version
  promises, eight per target, the three compose files and the two seed data
  files, and publishes only then, and only once both images are pushed. A
  draft missing any of them fails the check and stays a draft, so a
  half-assembled release is never visible. A pre-release is published with
  `--latest=false`, so it never becomes the repository's latest release.

## The build legs

Every leg is a reusable workflow (`on: workflow_call`). SLSA Build Level 3
requires that the signing material authenticating the provenance is out of
reach of the user-defined build steps, and every step of one job shares a
runner VM, so the build and its attestations run in a called workflow on its
own VM, whose steps the caller cannot add to. The Sigstore certificate then
names the called workflow as the signer, which a consumer can demand with
`--signer-workflow`.

**`release-build.yml`, per target:**

- builds `ferrofed` with `cargo auditable`, which embeds the dependency list in
  the binary's `.dep-v0` section, from a cold checkout with no cache;
- packages a flat `ferrofed-<tag>-<target>.tar.gz` (the binary, `LICENSE`,
  `NOTICE`, `README.md`) and its `.sha256sum`;
- writes two SBOMs: CycloneDX 1.5 from the source graph (`.cdx.json`) and SPDX
  from the shipped binary with syft (`.spdx.json`), failing when the binary
  carries no `pkg:cargo` purl;
- attests the tarball's SLSA provenance and both SBOMs with GitHub artifact
  attestations, and attaches the three Sigstore bundles
  (`.sigstore.json`, `.sbom.sigstore.json`, `.build-sbom.sigstore.json`) and the
  in-toto envelope (`.intoto.jsonl`) to the draft.

**`release-image.yml`, once:**

- takes the two musl tarballs from this run's artifacts and verifies each
  against `release-build.yml`'s signer before extracting it;
- builds the `linux/amd64` and `linux/arm64` index from `docker/Dockerfile` with
  no cache and no QEMU, and pushes it by digest, tagged `<version>` and, for a
  release that is not a pre-release, `<major>.<minor>` and `latest`;
- attests SLSA provenance for the index and each platform manifest and an SPDX
  SBOM per platform, all pushed to the registry as OCI referrers, then
  verifies the published image the way a consumer would.

**`release-viewer.yml`, once:**

- builds the musl `ferrofed-viewer` binary with `cargo auditable` on a runner
  of each architecture, and the architecture-independent site bundle with
  cargo-leptos through `scripts/release/viewer-site.sh`, each from a cold
  checkout with no cache, and attests the SLSA provenance of each binary and
  of the bundle's tarball;
- verifies all three against its own signer before it stages them for
  `docker/viewer/Dockerfile`;
- builds, pushes, attests and verifies the `linux/amd64` and `linux/arm64`
  index exactly as `release-image.yml` does for the gateway, with the same
  provenance and per-platform SPDX SBOM referrers and the same `pkg:cargo`
  purl check.

Verify a release:

```sh
gh attestation verify ferrofed-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-build.yml
gh attestation verify oci://ghcr.io/ferrohealth/ferrofed:X.Y.Z \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-image.yml
gh attestation verify oci://ghcr.io/ferrohealth/ferrofed-viewer:X.Y.Z \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-viewer.yml
```

The tools are pinned in `docs/VERSIONS.md` (`cargo-auditable`,
`cargo-cyclonedx`, `syft`, and `cargo-leptos` and the `wasm-bindgen` CLI for
the console) and `scripts/checks/versions.sh` holds the workflows to those
rows.

**A package's visibility is an owner setting.** GHCR creates
`ghcr.io/ferrohealth/ferrofed` and `ghcr.io/ferrohealth/ferrofed-viewer` on
their first push, private by default. After the first release pushes each,
the owner sets it public under the FerroHEALTH organization's package
settings and links it to the repository, so `docker pull` works without a
login.

The library crates are not part of this lane. `publish-crates.yml` runs on the
same `v*` tag and is described below (§ The crates.io lane).

Concurrency is `cancel-in-progress: false`. A second tag push queues behind the
first, because a release cancelled part-way through publishing is worse than a
slow one.

## Before the tag

1. **The milestone is empty.** `gh issue list --milestone vX.Y.Z --state open`
   answers nothing, or the owner calls the cut and moves the stragglers to the
   next milestone.
2. **The version moves in every file the pin matrix names:** `CITATION.cff`,
   the product-version row of `docs/VERSIONS.md` and the root `Cargo.toml`
   `[workspace.package]` `version`. The gateway image tag moves with it in
   the `compose.yaml` default, in the `FERROFED_VERSION` default on the
   `image:` line of `deploy/compose/compose.yaml` (the release asset) and in
   `deploy/kubernetes/statefulset.yaml`. `scripts/checks/versions.sh` fails on
   any file left behind, and the `plan` job checks the first three and the
   release asset's tag against the tag.
3. **The changelog names the release.** Every change since the last release
   is a fragment under `changelog.d/` (`changelog.d/README.md`), and entries
   written before the fragments may still sit under `[Unreleased]`. Run

   ```sh
   scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD
   ```

   in the version-bump branch. It writes a `## [X.Y.Z] - YYYY-MM-DD` section
   under a fresh empty `[Unreleased]`, holding the `[Unreleased]` entries and
   then the fragments, section by section, "Upgrade notes" first and then the
   Keep a Changelog order, and by file name within a section; it moves the `[Unreleased]` link reference on
   and adds the version's; and it `git rm`s the fragments. It refuses, with
   nothing written, a malformed fragment, a section heading that is neither
   "Upgrade notes" nor one Keep a Changelog defines, a version that already has a section, and a release with
   no entry. Commit `CHANGELOG.md` and the removals together. What sits under
   the version heading is what the release notes say, so read it as the
   release notes before you tag, and edit the wording there if it needs it.
   Its "Upgrade notes" must say everything an operator changes to upgrade:
   CI's `release compose` job already refuses a change whose build refuses
   the last release's example configuration with no upgrade note pending
   (`scripts/checks/upgrade-notes.sh`), and the book's Upgrading page
   (`website/book/src/operate/upgrading.md`) takes the release's notes in the
   same pull request.
   The landing page's release note and status panel
   (`website/landing/index.html`) name the same version in the same pull
   request: `scripts/checks/versions.sh` fails while they name an older one. The page is deployed from `main`, so the bump pull
   request is where it changes; the release lane never writes to `main`.
4. **The public texts are reviewed against Regulation (EU) 2025/327 Art 28.**
   Every public text is read for a claim the release does not ship, a
   limitation it leaves out, or a use outside the intended purpose, and the
   [claims review](https://ferrofed.eu/docs/evaluate/claims-review.html) gets
   a row for the release, in the version-bump pull request. Its method is on
   that page.
5. **The version bump lands as its own pull request** and merges like any
   other: the tier-1 gates (zizmor, actionlint, shellcheck, hadolint, comment
   style, file length, versions, changelog), the tier-2 Rust lanes and the
   `contribution-licence-guard` are green on it (`docs/ci-cd.md`).

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

1. The changelog needs a `## [0.0.1-rc.1]` section, which
   `scripts/release/changelog.sh --assemble 0.0.1-rc.1 <date>` writes, and
   `CITATION.cff` and the product row of `docs/VERSIONS.md` must say
   `0.0.1-rc.1`, because the `plan` job checks the tag against them. The rehearsal therefore goes through its
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

## The crates.io lane

The library crates under `crates/` (`openehr-federation`, `ihe-iti`,
`nl-generic-functions`, `oauth-server-metadata`) publish behind one switch: the
root `Cargo.toml` sets `[workspace.package] publish = false`, every `crates/*`
member inherits it, and
`app/*` and `tools/*` carry a hard `publish = false` of their own
(`docs/architecture.md` section 11, decision A35).

`publish-crates.yml` runs on every `v*` tag:

1. **select and package** checks that the tag names the workspace version,
   reads the publishable set from `cargo metadata`, writes it to the job
   summary, and packages every library crate with `cargo package`. While the
   switch is `false` the set is empty, the summary says nothing is published,
   and the run is a successful no-op.
2. **publish** runs only when the set is not empty, in the `crates-io`
   environment: it exchanges the workflow's OIDC identity for a short-lived
   crates.io token (Trusted Publishing), uploads each crate in dependency order
   at its manifest version, and reads the registry back. "Already exists"
   counts as done, so a run that failed part-way is finished by running it
   again (a dispatch with `publish` set, from `main` or the tag).

The `publish-dry-run` job of `ci.yml` packages the same crates on every pull
request, and the crate-version guard refuses packaged content that changes
without a version bump, because a published version is immutable
(`.claude/rules/crates-publishing.md`).

**Turning publishing on** is two owner steps and one line:

1. Create the `crates-io` GitHub environment, with the owner as required
   reviewer and a deployment policy for `main` and `v*` tags.
2. On crates.io, give each crate one Trusted Publisher entry: repository
   owner `FerroHEALTH`, repository `FerroFED`, workflow `publish-crates.yml`,
   environment `crates-io`. `openehr-federation`, `ihe-iti` and
   `nl-generic-functions` already exist (the 0.0.0 placeholders of
   2026-10-01), so they need no first upload with a personal token;
   `oauth-server-metadata` has no placeholder yet and needs one first.
3. Set `publish = true` in the root `[workspace.package]` in a pull request.
   The next `v*` tag publishes every library crate at its manifest version.

## After the tag

1. **Read the published release.** Its notes are the changelog section, and its
   asset list is what `finalize-release` verified. Run the two `gh attestation
   verify` commands of § The build legs against one tarball and the image.
2. **Post the board status update** with what shipped and what the next
   milestone targets (`.claude/rules/project-board.md`).
3. **Close the milestone.** `gh api -X PATCH
   repos/FerroHEALTH/FerroFED/milestones/<number> -f state=closed`, once the
   release is published. A milestone left open after its cut still reads as
   pending work on the roadmap board.
4. **Bring the standing instructions up to date.** `CLAUDE.md`'s status names
   the released version and the milestone now being built, in a pull request
   of its own, never inside the version bump.

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

- SLSA v1.2 build requirements: <https://slsa.dev/spec/v1.2/build-requirements>
- Artifact attestations and reusable workflows for SLSA Build Level 3:
  <https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-and-reusable-workflows-to-achieve-slsa-v1-build-level-3>
- cargo-auditable: <https://github.com/rust-secure-code/cargo-auditable>
- Available rules for rulesets:
  <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- Immutable releases:
  <https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases>
- Managing releases:
  <https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository>
- GitHub Actions security hardening:
  <https://docs.github.com/en/actions/security-for-github-actions/security-hardening-for-github-actions>
