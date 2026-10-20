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
to `main`, on a pull request, or in a merge group. A tag on a security patch
branch runs it the same way as a tag on `main` (§ Cutting a security patch
release). `workflow_dispatch` re-runs
it for a tag that already exists and has to be dispatched at that tag.

```text
plan ── github-release (draft) ── build-binaries ── build-image ── finalize-release (publish) ── milestone
                               └─ build-viewer ───────────────┘
```

- **plan** validates the tag shape, refuses a dispatch that is not at the tag
  it names, refuses the tag when no milestone is titled exactly as it or that
  milestone still holds an open issue or pull request (naming each one),
  decides with `scripts/release/latest.sh` whether the release
  becomes the latest (a release with no pre-release suffix whose version no
  `vX.Y.Z` tag of the repository exceeds), checks the tag against every
  file that declares the product
  version, and extracts the `## [X.Y.Z]` section of `CHANGELOG.md` as the
  release notes. A tag whose tree has no root `Cargo.toml` fails here, since
  the tag cannot be checked against the workspace version. A missing or empty
  changelog section fails the release, so a cut can never ship with notes
  generated from the commit range standing in for the changelog. It then
  runs `scripts/checks/technical-documentation.sh`, which refuses a release
  that changes a harmonised component with no review row for it (§ Before
  the tag, step 6), and `scripts/checks/annex-ii.sh`, which refuses an
  Annex II checklist with a point left out or evidence that does not exist,
  before any release exists. Last it reads the
  manufacturer's file of the EU declaration of conformity,
  `conformance/eu-declaration-of-conformity.toml` (§ Before the tag, step
  7): when it issues the declaration, `plan` writes the declaration once
  and fails on any value left empty; when it does not, `plan` warns that
  the release carries none.
- **github-release** creates the release as a draft carrying those notes, and
  attaches `deploy/compose/compose.yaml`, `ferrofed.toml` and `registry.toml`
  under those names: the gateway alone at this release's image, with the two
  example files it mounts. It attaches the nginx reverse proxy
  configuration of the production guide, `deploy/nginx/ferrofed.conf`, as
  `ferrofed.conf`, with its SHA-256 as `ferrofed.conf.sha256sum`, and fails
  when the tag's tree has no such file. It also attaches the conformance
  seed data `scripts/release/seed-data.sh` writes,
  `ferrofed-conformance-seed-data.json` (the vendored demo data
  `ferrofed conformance run --seed-data` reads, with its Apache-2.0 licence
  and notice) and its SHA-256 as
  `ferrofed-conformance-seed-data.json.sha256sum`, and the Grafana dashboard
  and Prometheus alert rules of `deploy/observability/`,
  `ferrofed-dashboard.json` and `ferrofed-alerts.yaml`. It attaches the two
  documents Regulation (EU) 2025/327 Art 30(1)(d) has the system accompanied
  by, as the tag's Markdown source, which a screen reader reads as text:
  `ferrofed-vX.Y.Z-information-sheet.md`
  (`website/book/src/evaluate/information-sheet.md`) and
  `ferrofed-vX.Y.Z-instructions-for-use.md`
  (`website/book/src/operate/instructions-for-use.md`).
  `scripts/release/accompanying-documents.sh` writes both and refuses an
  information sheet that does not say "FerroFED X.Y.Z, released on", so a
  sheet left at the previous release fails here. When `plan` found the
  declaration issued, it attaches the EU declaration of conformity of
  Regulation (EU) 2025/327 Art 39, with every item of Annex IV,
  `ferrofed-vX.Y.Z-eu-declaration-of-conformity.md`, which
  `scripts/release/declaration-of-conformity.sh` writes from the tag's
  records and the manufacturer's file. A draft is mutable and
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
  promises, eight per target, the three compose files, the nginx
  configuration and its checksum, the two seed data files, the dashboard
  and alert rules, the information sheet and instructions for use of
  the tag, and the EU declaration of conformity when `plan` found it
  issued, and publishes only then, and only once both images are
  pushed. A draft missing any of them fails the check and stays a draft, so a
  half-assembled release is never visible. A release `plan` did not find the
  latest, such as a pre-release, is published
  with `--latest=false`, so it never becomes the repository's latest
  release.
- **milestone** closes the milestone titled as the tag once every other job
  has succeeded, with the workflow token and job-level `issues: write`. When a
  leg fails the job still runs, names the leg in its log, and leaves the
  milestone open; re-running the failed jobs closes it. An already-closed
  milestone counts as done, and a pre-release finishes no milestone, so
  neither milestone step reads or changes one for an `-rc.N` tag. Both call
  `scripts/release/milestone.sh`. The crates.io lane is `publish-crates.yml`,
  a workflow of its own (§ The crates.io lane), which no `needs` edge of
  `release.yml` reaches.

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
  no cache and no QEMU, and pushes it by digest, tagged `<version>`, for a
  release that is not a pre-release `<major>.<minor>`, and `latest` when
  `plan` found the release the latest;
- labels and annotates it with `eu.ferrofed.information-sheet` and
  `eu.ferrofed.instructions-for-use`, the download URLs of the two documents
  the release attaches, so an image names the copies that describe it;
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
  two document labels, the same provenance and per-platform SPDX SBOM
  referrers and the same `pkg:cargo` purl check.

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
   next milestone. The `plan` job refuses the tag otherwise, and
   `GITHUB_REPOSITORY=FerroHEALTH/FerroFED bash scripts/release/milestone.sh
   check vX.Y.Z` asks the same question before you tag.
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

   **The support period.** For a release without a pre-release suffix, the
   same command states the support period, which Regulation (EU) 2024/2847
   Art 13(8) and (19) and Annex II point 7 ask for. The section opens with
   "Security fixes until YYYY-MM-DD", the release date plus five years, the
   minimum Art 13(8) sets, and says that fixes ship only in the newest
   release (Art 13(10)), so the release is supported while it is the
   newest; it adds the row `| vX.Y.Z | <date> | <end> |`
   above the support table's marker in `SECURITY.md`; and it prints the end
   date. Commit `SECURITY.md` with `CHANGELOG.md`. It refuses, with nothing
   written, a `SECURITY.md` with no support table and a version the table
   already lists. The date given to `--assemble` is the release date both
   files carry, so give it the day the tag will be pushed. A pre-release
   gets no support period. Five years is the number `SUPPORT_YEARS` in
   `scripts/release/changelog.sh`; a longer period the owner sets changes
   it, `SUPPORT_YEARS` in `app/ferrofed-server/src/support.rs`, the
   support-period check of `scripts/checks/versions.sh`, the text of
   `SECURITY.md` and this paragraph together. The release build reads the
   date of the version's heading in `CHANGELOG.md` and compiles it into the
   binary as `FERROFED_RELEASE_DATE`, so the gateway names its own end date
   and says when it has passed; the build fails when the heading has no date.
   `scripts/checks/versions.sh` fails when a release in `CHANGELOG.md` has no
   row in the support table, or a row whose end date is not five years after
   the release date.
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
5. **The information sheet, the instructions for use and the clinical
   safety risk file describe the release.** In the version-bump pull
   request, the
   [information sheet](https://ferrofed.eu/docs/evaluate/information-sheet.html)
   (`website/book/src/evaluate/information-sheet.md`) names the release's
   date and adds it to its release table, and its standards table matches
   what the release supports; `scripts/checks/versions.sh` fails while its
   product version, a pin or the date differs from `docs/VERSIONS.md` and
   `CHANGELOG.md`. The
   [instructions for use](https://ferrofed.eu/docs/operate/instructions-for-use.html)
   list every maintenance step and limitation the release adds. When the
   release changes the `ehds-logging` pin, `crates/eehrxf`, the access log of
   `app/ferrofed-server/src/access/`, a mapping pin or the query path they
   ride, the
   [clinical safety risk file](https://ferrofed.eu/docs/evaluate/clinical-safety.html)
   is reviewed and its review table gets a row. The release attaches both
   pages as they stand at the tag, so what the bump merges is the copy an
   operator downloads (§ What the lane does). The
   [cybersecurity risk assessment](https://ferrofed.eu/docs/evaluate/cra-risk-assessment.html)
   is reviewed against what the release changes and gets a line in its
   revisions table. `SECURITY.md` is the one record of the support periods,
   and the table of end dates under point 7 of
   [CRA information to the user](https://ferrofed.eu/docs/evaluate/cra-user-information.html)
   (`website/book/src/evaluate/cra-user-information.md`) is its copy: paste
   the row `changelog.sh --assemble` added to `SECURITY.md` into it, as it
   stands, in the same commit. `scripts/checks/support-periods.sh` fails the
   bump, and the `plan` job at the tag, while the two tables differ.
6. **A change to a harmonised component is reflected in the technical
   documentation** (Regulation (EU) 2025/327 Art 30(2)). When the pin of the
   published `ehds-logging` crate moves, or a file under `crates/eehrxf`,
   `app/ferrofed-eehrxf`, `app/ferrofed-server/src/access` or
   `app/ferrofed-server/src/fhir` differs from the previous release tag, the version-bump pull request
   adds a row whose release is the new version to the clinical safety
   review table and a row for it to "Changes to the harmonised components"
   of the
   [technical documentation](https://ferrofed.eu/docs/evaluate/technical-documentation.html)
   (`website/book/src/evaluate/technical-documentation.md`), and brings its
   Annex III index (`conformance/technical-documentation.tsv`) up to date.
   `scripts/checks/technical-documentation.sh` fails the bump, and the
   `plan` job at the tag, while either row is missing. Between releases the
   workspace version is the last release's, whose rows exist, so a routine
   pull request that changes a component passes and the next bump records
   it.
7. **The EU declaration of conformity is issued, or the release carries
   none.** Regulation (EU) 2025/327 Art 39 has the manufacturer draw up
   the declaration with the content of Annex IV. The repository's records
   fill most of it; the items only the manufacturer can supply are the
   values of `conformance/eu-declaration-of-conformity.toml`: the place
   of issue, the name and function of the person who signs, on whose
   behalf, the signature, the harmonised standards, the other Union acts
   the declaration covers (Art 39(2)) and the result from the testing
   environment of Art 40. While the file says `issue = false`, a release
   carries no declaration, and the lane says so in a warning. Once the
   owner fills every value and sets `issue = true`, every release cut from
   that tree carries one, and a value left empty fails the `plan` job
   before the release exists, so no release ships a declaration with a
   blank. Read the declaration before the tag with

   ```sh
   scripts/release/declaration-of-conformity.sh --draft X.Y.Z "$(git rev-parse HEAD)" /tmp/doc
   ```

   which writes the same document with each missing value marked.
8. **The version bump lands as its own pull request** and merges like any
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
   own version-bump pull request, exactly like a cut, with review rows for
   `0.0.1-rc.1` when it changes a harmonised component (§ Before the tag,
   step 6).
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
`nl-generic-functions`, `oauth-server-metadata`, `ehds-logging`, `eehrxf`) publish behind one switch: the
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
   2026-10-01), and so do `eehrxf` and `ehds-logging` (those of
   2026-10-06), so they need no first upload with a personal token;
   `oauth-server-metadata` has no placeholder yet and needs one first.
3. Set `publish = true` in the root `[workspace.package]` in a pull request.
   The next `v*` tag publishes every library crate at its manifest version.

## After the tag

1. **Read the published release.** Its notes are the changelog section, and its
   asset list is what `finalize-release` verified. Run the two `gh attestation
   verify` commands of § The build legs against one tarball and the image.
2. **Post the board status update** with what shipped and what the next
   milestone targets (`.claude/rules/project-board.md`).
3. **The milestone is closed.** The `milestone` job of `release.yml` closes
   it once every leg has succeeded, because a milestone left open after its
   cut still reads as pending work on the roadmap board. When a leg fails,
   the job names it and leaves the milestone open, and a re-run of the failed
   jobs closes it. After a manual or recovery release, check that it is
   closed, and close it with `GITHUB_REPOSITORY=FerroHEALTH/FerroFED bash
   scripts/release/milestone.sh close vX.Y.Z` when it is not.
4. **Bring the standing instructions up to date.** `CLAUDE.md`'s status names
   the released version and the milestone now being built, in a pull request
   of its own, never inside the version bump.
5. **Archive the release for ten years.** Regulation (EU) 2025/327 Art 30(3)
   keeps the technical documentation and the EU declaration of conformity
   for 10 years after the release is placed on the market. The owner keeps
   the archive copy the
   [technical documentation](https://ferrofed.eu/docs/evaluate/technical-documentation.html#keeping-it-for-ten-years)
   names, outside GitHub: the tag's bundle, the release assets and the
   conformance and FHIR validation artifacts of the CI run on the release
   commit, which GitHub deletes when its artifact retention ends. The
   release assets include the EU declaration of conformity when the
   release carries one.

## Cutting a security patch release

Only the newest release is supported (`SECURITY.md`), so a security fix
ships in a new release, never in an older one: there are no maintenance
lines and no backports. The fix ships as a patch release of the newest
release with no feature changes in it, so the update is provided separately
from functionality updates (Regulation (EU) 2024/2847 Annex I Part II(2)).
`main` carries feature work, so a security patch is never tagged on `main`.
It is cut from the newest release's tag on a short-lived patch branch, and
only its records come back to `main`.

The patch is the newest release's version with its patch number one
higher, cut from that release's tag, so it carries the fix and nothing
`main` merged since. That number is the one `main`'s next release was
planned under, so step 1 moves that milestone on by one.

1. **Name the patch and its branch.** The newest release's tag is the
   highest `vX.Y.Z` with no pre-release suffix, and the patch is that
   version with its patch number one higher:

   ```sh
   git fetch origin --tags
   git tag --list 'v0.0.*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n1
   ```

   The patch branch is `release/<major>.<minor>.x`, a `release/` branch
   of the conventional types that the `release/vX.Y.Z` version-bump
   branches never match, and it starts at the newest release's tag:

   ```sh
   git branch release/0.0.x v0.0.10
   git push origin release/0.0.x
   ```

   A branch an earlier security patch left must hold the newest release's
   tag (`git merge-base --is-ancestor v0.0.10 origin/release/0.0.x`). A
   later release may have been cut from `main` since; delete the branch and
   push it again from the newest release's tag, because the earlier patch
   keeps its own tag. The patch gets its own milestone. When a milestone
   already carries the patch's number, as `main`'s next milestone always
   does, rename that one to the number after it first (`gh api -X PATCH repos/FerroHEALTH/FerroFED/milestones/<number>
   -f title=v0.0.12`).
2. **The branch carries this procedure's tooling.** A pull request into the
   branch runs CI only when the branch's own `ci.yml` lists
   `release/*.*.x`, and the tag's lane leaves `latest` alone only when the
   branch's `release.yml` runs `scripts/release/latest.sh`. A release whose
   tag predates both (#1087) gets that change cherry-picked first, in a `ci/`
   pull request into the branch with nothing else in it. It changes no
   product behaviour, so the patch still carries no feature.
3. **The fix lands on `main` first,** as an ordinary pull request with a
   `security` fragment, `changelog.d/<issue>-<slug>.security.md`
   (`changelog.d/README.md`). Its `Closes #N` closes the issue: the keyword
   closes nothing in a pull request into any other branch. While the
   advisory is private, a public pull request discloses the vulnerability,
   so the owner decides when the fix goes public.
4. **Cherry-pick only the fix** onto a branch from the patch branch,
   and open a pull request into it:

   ```sh
   git switch -c fix/<slug>-0.0 origin/release/0.0.x
   git cherry-pick -x <the fix's squash commit on main>
   git push origin fix/<slug>-0.0
   gh pr create --base release/0.0.x
   ```

   The cherry-pick brings the fragment with it. Nothing else from `main`
   goes onto the branch: when the fix conflicts with code the release does
   not have, resolve it against the release's own code, and when the fix
   needs a feature the release lacks, write the smallest fix of the
   release's code and say so in the pull request. CI runs on the pull request as on one into
   `main`, and `contribution-licence-guard` reads its body. The branch has
   no merge queue, so merge it once `conclusion` is green.
5. **The version bump lands on the branch** from a `release/vX.Y.Z` branch,
   in a pull request into `release/0.0.x`, following § Before the tag steps 2
   to 8 with the patch's version:
   - the version files move to the patch version;
   - `scripts/release/changelog.sh --assemble X.Y.Z YYYY-MM-DD` on the
     branch finds only the fix's fragments, so the section it writes holds
     the fix alone. It adds the patch's row to the support table of
     `SECURITY.md`, and the row is copied to the book's CRA information to
     the user as step 5 there says;
   - the landing page names the patch, because `scripts/checks/versions.sh`
     holds it to the newest release in the branch's `CHANGELOG.md`. The
     site is deployed from `main`, so this copy only satisfies the guard,
     and step 8 moves `main`'s;
   - the claims review, the information sheet, the instructions for use and
     the cybersecurity risk assessment get the patch's rows, and the
     technical documentation its review rows when the fix changes a
     harmonised component.
6. **Tag the branch's tip** and push the tag, as § The tag does for `main`:

   ```sh
   git fetch origin
   git tag -s vX.Y.Z -m "vX.Y.Z" origin/release/0.0.x
   git push origin vX.Y.Z
   ```

   `release.yml` runs the whole lane. `plan` checks the tag against the
   branch's version files and changelog, and `scripts/release/latest.sh`
   confirms the patch becomes the latest: it is the newest release, so it
   is published as the latest and its images get `<version>`,
   `<major>.<minor>` and `latest`. `publish-crates.yml` runs on the same tag
   and is the no-op it always is while the publish switch is off. Once the
   switch is on, a crate the fix changes needs a version on the branch that
   no other release has published, because crates.io refuses a version
   twice (`.claude/rules/crates-publishing.md`).
7. **After the tag** follows § After the tag, steps 1, 2 and 5, for the
   patch. The security advisory names the patch release as the patched
   version and every earlier release as affected.
8. **Merge the records back to `main`** in one `chore/` pull request, once
   the patch is published. `main` merges by squash, so the records are
   copied, never merged as a branch:
   - the patch's `## [X.Y.Z] - YYYY-MM-DD` section of `CHANGELOG.md`, placed
     in version order, with its link reference, and the `[Unreleased]`
     reference compares from its tag;
   - its row of the support table of `SECURITY.md`, and the copy of that
     row on the book's CRA information to the user;
   - the removal of the fix's fragment from `changelog.d/`, because the
     patch's section records the fix;
   - the landing page's release note and status panel, which
     `scripts/checks/versions.sh` holds to the newest release in
     `CHANGELOG.md`.

   `main`'s version files stay at the release they name, and its next
   version bump moves them past the patch.

**Rehearse it** with a pre-release tag on the patch branch
(`v0.0.11-rc.1`), as § Rehearsing a change to the lane describes for `main`.
`scripts/release/latest.sh --self-test` proves the latest decision over a
stub remote, and CI runs it.

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

## Withdrawing a release

A released version found not to conform is withdrawn when the corrective
action in its row of `docs/registers/non-conforming-versions.tsv` says so
(Regulation (EU) 2025/327 Art 30(1)(i); the procedure is
[`post-market.md`](post-market.md)). Nothing published is edited: the
release, its notes, its assets and its tag stay as they are, under the
immutable-releases setting and the `release-tags` ruleset. The withdrawal
is recorded beside it instead.

1. **The correcting release ships first,** as a patch release in the same
   `<major>.<minor>` line. Its tag moves `<major>.<minor>` and `latest`
   onto it in the normal way.
2. **Run the script dry, then for real:**

   ```sh
   scripts/release/withdraw.sh --version 0.0.9 --replacement 0.0.10 \
     --finding N-2026-1 --issue 812
   scripts/release/withdraw.sh --version 0.0.9 --replacement 0.0.10 \
     --finding N-2026-1 --issue 812 --apply
   ```

   `--issue` is the tracker issue of the finding, and names the upgrade
   note. For a vulnerability, add `--security-advisory GHSA-…`: the script
   appends the withdrawal to that advisory and publishes it. Without one,
   it writes `docs/advisories/v<version>-withdrawn.md`.

   It refuses a version with no row naming it in the register, a version
   SECURITY.md already lists as withdrawn, and a replacement that is not a
   later published release of the same line. With `--apply` it moves the
   `<major>.<minor>` and `latest` tags of `ghcr.io/ferrohealth/ferrofed` and
   `ghcr.io/ferrohealth/ferrofed-viewer` to the replacement wherever they
   still point at the withdrawn version. It then publishes the advisory,
   writes the upgrade note under `changelog.d/` that the next release's notes
   carry, lists the version in SECURITY.md, and records the action and the
   date users were told in the register row. Last, it prints the notices for
   the national authorities and for distributors, importers and users.
3. **Commit what it wrote** (the register, SECURITY.md, the fragment and any
   advisory page) in one pull request, and send the notices.

The `<version>` image tag stays. GHCR removes a tag only by deleting the
package version, which is the image digest itself. That would break every
deployment pinned by digest, and the attestations that name the digest
would no longer resolve. A deployment that pulls `<version>` or the digest
keeps running the withdrawn version until it moves. One that follows
`<major>.<minor>` or `latest` gets the replacement on its next pull.

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
