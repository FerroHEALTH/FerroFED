<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CI: the two tiers and the required checks

No specification governs this; it is FerroFED's own design, the one the
FerroHEALTH family shares, grounded in the OWASP GitHub Actions Security Cheat
Sheet, OpenSSF Scorecard and the zizmor audit set. The enforceable discipline
is `.claude/rules/ci-cd.md`, which this document does not repeat. What follows
is the design of the workflows under `.github/workflows/` and the reason each
is shaped the way it is.

## The problem

The repository started with no Cargo workspace, so a conventional CI workflow
would have had nothing to run, and a workflow added with the workspace would
have arrived after the files it is meant to guard. The workflows hold tokens,
the shell scripts under `scripts/` are the tracker helpers, the vendor scripts
and the committed guards, and `docs/specs/` holds the vendored corpora whose
pins must agree with `docs/VERSIONS.md`. All of that was live from the first
commit and needed a gate, so the CI was built in two tiers, and the Rust tier
has run on every change since the workspace landed.

## The workflows

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yml` | push to `main`, pull request, merge group, dispatch | the two tiers below and the `conclusion` check |
| `contribution-licence.yml` | pull request opened, edited, reopened or synchronized | `contribution-licence-guard`: the pull request body accepts the contribution terms |
| `codeql.yml` | push and pull request touching workflows, actions or Rust; Mondays | CodeQL in advanced setup; the Actions analysis runs now, the Rust analysis is gated on a root `Cargo.toml` |
| `scorecard.yml` | push to `main`, a branch-protection change, Mondays | OpenSSF Scorecard, results uploaded to code scanning |
| `sonar.yml` | push to `main`, same-repository pull requests | SonarQube Cloud, advisory; the Rust coverage steps are gated on a root `Cargo.toml` |
| `docs.yml` | push to `main`, pull request, dispatch | builds the site (the landing page at `/`, the book under `/docs/`) on every event and deploys it to GitHub Pages from `main` only |
| `pin-freshness.yml` | Mondays, dispatch | the pins nothing else watches, compared with upstream; one issue when one is behind |
| `release.yml` | a pushed `v*` tag, dispatch at a tag | the release lane: tag checked against the declared version, changelog section as the notes, draft then publish, with a tag whose tree has no root `Cargo.toml` refused at `plan` (`docs/release.md`) |
| `release-build.yml` | called by `release.yml`, once per target | the SLSA Build Level 3 binary lane: `cargo auditable` build, CycloneDX and syft SBOMs, provenance and SBOM attestations, every asset attached to the draft (`docs/release.md` § The build legs) |
| `release-image.yml` | called by `release.yml` | the container from the attested musl binaries, pushed to `ghcr.io/ferrohealth/ferrofed` by digest with provenance and SBOM attestations as OCI referrers, verified as a consumer would |
| `publish-crates.yml` | a pushed `v*` tag, dispatch | the crates.io lane behind the workspace `publish` switch: the publishable set from `cargo metadata`, packaged, then uploaded in dependency order through Trusted Publishing; a successful no-op while the switch is `false` (`docs/release.md` § The crates.io lane) |
| `fuzz.yml` | Wednesdays, dispatch, and pull requests touching `crates/openehr-federation`, `app/ferrofed-server`, `fuzz/` or `scripts/fuzz/` | the four `cargo fuzz` targets over the untrusted inputs, time-boxed and advisory, after a check that the generated seeds are current (§The fuzz lane) |

Dependabot (`.github/dependabot.yml`) is the thirteenth piece and is described
under the pins below.

## The two tiers of `ci.yml`

`ci.yml` splits on whether a check needs Rust.

**Tier 1 needs no Rust toolchain.**

| Job | Runs |
|---|---|
| `zizmor` | `zizmor --min-severity=low .github/`, with `GH_TOKEN` so the online audits work |
| `actionlint` | the official image, pinned by tag and digest |
| `shellcheck` | `--severity=style` over every tracked `*.sh` and every tracked extensionless file with a shell shebang, outside `docs/specs/**` and `**/vendor/**` |
| `hadolint` | every tracked Dockerfile under `.hadolint.yaml`, outside the vendored trees, which today is `docker/Dockerfile` |
| `comment-style` | `scripts/checks/comment-style.sh --all` |
| `file-length` | `scripts/checks/file-length.sh`, the 1000-line cap on hand-written Rust with its ratchet allow-list |
| `versions` | `scripts/checks/versions.sh --self-test`, then `scripts/checks/versions.sh`: the pin matrix against every file that repeats a pin and each specification row against the crate constant it names, the landing page's release string against the newest `CHANGELOG.md` release, the book's pin table against the rows it names, the vendored provenance stamps and the SPDX licence claims |
| `favicon-sync` | `scripts/checks/favicon-sync.sh`, the book theme favicons byte-identical to the brand favicon set |
| `conformance-matrix` | `scripts/checks/conformance-matrix.sh`, the conformance matrix against the vendored specification, the test markers against the matrix, the rendered book page against the matrix, and the README conformance badges under `conformance/badges/` against the matrix and the AQL golden pass list (`docs/architecture.md` section 12) |
| `tracker-helpers` | the `--self-test` of `scripts/gh/fields.sh`, `labels.sh`, `migrate-fields.sh` and `rel.sh`, each driven against a stub `gh` on `PATH` |

The vendored trees are excluded from shellcheck and hadolint on purpose. The
reference implementation ships its own shell scripts, Dockerfile and
workflows; they are evidence, kept byte for byte as published
(`.claude/rules/vendored-inputs.md`), and linting them would either fail the
build on code that is not ours or invite an edit that breaks the provenance
digest. zizmor and actionlint read only the root `.github/`, so the nested
workflows under `docs/specs/` are never audited as if they ran here.

`versions` skips each comparison whose subject file is absent and reports the
skip with its reason. Every subject file it reads exists today, the
architecture pin rows in `docs/architecture.md`, the workspace rows in the root
`Cargo.toml` and the release tool pins in `release-build.yml` and
`release-image.yml`, so every comparison runs.

**Tier 2 is gated on the workspace.** A `detect` job checks out and looks
for a root `Cargo.toml`, publishing a boolean output. Every Rust job carries
`needs: detect` and `if: needs.detect.outputs.cargo == 'true'`: rustfmt,
clippy at `-D warnings`, nextest plus doctests, rustdoc, `cargo deny check`,
MSRV through `cargo hack check --rust-version`, and `dependency-review` on
pull requests. Each lane mirrors the local command in `.claude/rules/ci-cd.md`.
The lanes were written before the workspace, so the workspace pull request
changed nothing in CI and the lanes activated by themselves; they have run on
every change since.

`e2e (containers)` is the Rust lane that needs Docker. A container-backed test
checks the `FERROFED_E2E` gate first and returns early without it, so the
`test` job stays offline and fast; this job sets `FERROFED_E2E=1` and runs the
container tests (the whole `tools/ferrofed-testkit` suite and the `e2e` module
of `app/ferrofed-server`) against the digest-pinned images of
`docs/VERSIONS.md` §Container images: two FerroEHR instances as the two nodes,
each behind the testkit's capturing and fault proxy (`docs/architecture.md`
§13). It feeds `conclusion` like every other lane. Locally:
`FERROFED_E2E=1 cargo nextest run -p ferrofed-testkit -p ferrofed-server -E
'test(/^e2e::/)'` with Docker running.

`features (cargo-hack)` lints every feature of the three published crates on
its own: `cargo hack clippy --each-feature --all-targets` over
`openehr-federation`, `ihe-iti` and `nl-generic-functions`, at `-D warnings`.
Each of those crates is one specification with a feature per layer or profile
(`docs/architecture.md` §11), so a feature that only builds beside another one
is a defect a caller would hit; the workspace `clippy` job sees the
all-features union only. The job runs per package, never over the workspace.

`crate-version-guard` runs on pull requests only and fails a change that
alters a `crates/*` member's packaged content without moving its version,
because a published version is immutable (`.claude/rules/crates-publishing.md`).
It exits cleanly while no `crates/*` member exists, and the `no-crate-bump`
label is its escape for a diff that provably does not change packaged bytes.
Nothing is published yet, behind the workspace `publish` switch
(`.claude/rules/crates-publishing.md`); the guard keeps each crate's line
honest until the switch flips.

`hashFiles()` cannot do the detection. It is evaluated before checkout, when
the workspace is empty, so an `if: hashFiles('Cargo.toml') != ''` gate on a job
never sees the file. A detection job that checks out and tests for the file is
the correct primitive, and `codeql.yml` uses the same one for its Rust
analysis. `sonar.yml` can use `hashFiles()` because its gates are step-level,
after the checkout.

## Why a mostly-skipped pipeline is still enforceable

A ruleset requires status checks by name, and a job GitHub reports as
`skipped` satisfies a required check without having verified anything. Ten
skipped Rust lanes named individually in the ruleset would read as ten green
checks over a tree nobody compiled, and re-listing the checks would be an
owner action every time a job is added or renamed.

The `conclusion` job removes both problems. It `needs` every other job in
`ci.yml`, runs under `if: always()` so it reports even when its dependencies
were skipped, and reads `join(needs.*.result, ' ')` through `env:`. It fails
when any result is `failure` or `cancelled`, and passes when every result is
`success` or `skipped`. A job added to `ci.yml` must join the `conclusion`
job's `needs` list in the same change, or its result is not counted.

Reading the results through `env:` rather than splicing them into `run:` is
the same template-injection rule every workflow follows
(`.claude/rules/ci-cd.md`).

## The two required checks

The `main` ruleset requires exactly two status checks:

- **`conclusion`**, the verdict over the whole of `ci.yml`, for the reason
  above. It is the name that never changes as jobs come and go.
- **`contribution-licence-guard`**, from `contribution-licence.yml`. A
  contribution is licensed under the project licence and grants the Licensor
  the relicensing right (`CONTRIBUTING.md` § Licensing of contributions); the
  ticked checkbox in the pull request body is the record of acceptance, and
  this check is what makes it binding. It stays a workflow of its own rather
  than a `ci.yml` job because it reacts to a different event: editing the
  pull request body re-runs the guard without re-running the build. The body
  reaches `scripts/checks/contribution-licence.sh` through `env:`, because a
  pull request body is attacker-controlled text. A bot author is skipped by
  author type, since a bot cannot accept terms, and a skipped job satisfies
  the required check.

No other check joins the list. A new `ci.yml` job joins `conclusion`'s
`needs`; a new workflow that must gate a merge is a decision recorded here
first.

## Every pin, and what watches it

A pin nothing watches goes stale silently, so each class names its mechanism.

| Pin | Watched by |
|---|---|
| `uses:` references in `.github/workflows/**` and `.github/actions/**`, pinned by full commit SHA | Dependabot, `github-actions` ecosystem |
| the workspace dependency table in the root `Cargo.toml`, with the `openehr-*` family as one lockstep group | Dependabot, `cargo` ecosystem |
| a digest-pinned `FROM` in a first-party Dockerfile | Dependabot, `docker` ecosystem at `/` and `/docker` |
| the zizmor, actionlint, shellcheck and hadolint versions in `ci.yml` | `pin-freshness.yml`, weekly |
| the Federation Tier specification and reference implementation commits | `pin-freshness.yml`, weekly, against each repository's `main` |
| the e2e node images, by tag and digest in the testkit's `PinnedImage` constants | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` image rows; a bump is a deliberate change to both |
| the release and fuzz tool versions (`cargo-auditable`, `cargo-cyclonedx`, `syft`, `cargo-fuzz`) | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` tool rows |
| the fuzz seeds generated from the vendored corpora (`fuzz/seeds/*/gen-*`) | `scripts/fuzz/seeds.sh --check`, the first job of `fuzz.yml` |
| `fuzz/Cargo.lock` against the workspace crates the fuzz targets depend on | the `fuzz lockfile` job of `ci.yml`, on every pull request (`cargo metadata --locked`) |
| every pin repeated in a second file | `scripts/checks/versions.sh` against `docs/VERSIONS.md` |

Every Dependabot ecosystem carries a 7-day cooldown. CI is where the
release-signing identity will live, so a compromised action release is the
attack the cooldown buys detection time against; security updates are exempt
from cooldown by design and still arrive at once.

**Dependabot does not read an analyzer version** inside a `run:` block or an
installer input, nor a commit a vendor script fetches. `pin-freshness.sh`
reads those pins from `docs/VERSIONS.md`, compares each with the newest
upstream release or commit, and opens one issue carrying the report when one
is behind, adding nothing when an open issue already carries it. The issue
goes through `scripts/gh/fields.sh new` as a Task at Low priority and Low
effort; the job's default token may not read the organisation's issue types,
and then the issue lands with its `ci` label alone and whoever picks it up
sets the type, the priority and the effort. A pin it
cannot read fails the job, so a network failure never reads as a fresh pin.
It opens an issue rather than failing red, because a weekly red job on a lint
version trains a maintainer to ignore red jobs. For the specification the
issue is the trigger for a re-pin, which is never automatic: a new commit can
change a requirement, and a re-pin re-checks every cited N and CP
(`.claude/rules/spec-adherence.md`).

## The analyzers are advisory

CodeQL, Scorecard and SonarQube Cloud report into code scanning and the Sonar
project. Their findings are triaged against the specifications and the
project rules (`.claude/rules/ai-code-review.md`), and none of them gates a
merge. SonarQube Cloud runs from CI with Automatic Analysis off, because the
two cannot both be on for one project. Its scope, including the exclusion of
`docs/specs/**` and `**/vendor/**`, lives in `sonar-project.properties`.

## Why the GitHub licence field reads NOASSERTION

GitHub detects a repository licence with licensee, whose licence set does not
contain BUSL-1.1, so `gh api repos/FerroHEALTH/FerroFED --jq .license`
returns `NOASSERTION` and will keep returning it. No layout of `LICENSE` can
change that, and changing a term to satisfy a detector would be a change to
the licence. The terms are in `LICENSE` and `NOTICE`, and every first-party
file carries an `SPDX-License-Identifier: BUSL-1.1` header that
`scripts/checks/versions.sh` enforces.

## The fuzz lane

`.github/workflows/fuzz.yml` runs the four `cargo fuzz` targets of the `fuzz/`
crate over the inputs a caller controls before anything else reads them (#134):

- **`aql_rewrite`:** the rewrite over AQL text and parameters;
- **`adhoc_query`:** the ITS-REST `AdhocQueryExecute` body through the
  façade's intake;
- **`result_set_meta`:** a federated `RESULT_SET` and its `meta.federation`;
- **`options_root`:** the `OPTIONS {base}/` body.

It runs every Wednesday and on dispatch for five minutes per target, and for
one minute per target on a pull request that touches the code a target reads.
It is a time-boxed search, so it is never a `conclusion` input.

A panic, an abort or a hang is a defect: the run fails, uploads the
reproducing input as an artifact for 90 days, and the finding becomes a `Bug`
issue with the input attached. An `Err` or a refusal is never a finding.
`aql_rewrite` also asserts the identifier-hygiene property of §5.4.1 (N33) on
every query the rewrite accepts, so a node query that still carries the
patient identifier, in a literal or rebuilt by a string function, fails the
run like a crash (`fuzz/README.md`).

A first job runs `scripts/fuzz/seeds.sh --check`: the seeds are generated from
the vendored golden cases and the specification's JSON examples, and a
re-vendored corpus that leaves them stale fails it. The lane is the one job on
a nightly toolchain, through the `toolchain` input of the `setup-rust`
composite, because cargo-fuzz needs sanitizer flags stable does not carry. The
`fuzz/` crate is its own workspace, excluded from the root one, so the product
stays on the pinned stable toolchain. Its `cargo-fuzz` pin is a
`docs/VERSIONS.md` row the versions guard checks.

Because `fuzz/` keeps its own `Cargo.lock`, a workspace change that moves a
crate the targets depend on can leave that lockfile stale while every
workspace job stays green. The `fuzz lockfile` job of `ci.yml` resolves it
with `--locked` on every pull request and feeds `conclusion`, so the change
that makes it stale refreshes it in the same pull request (#143).

## Triggers and concurrency

`ci.yml` runs on `push` to `main`, `pull_request` against `main`,
`merge_group` and `workflow_dispatch`. `cancel-in-progress` is true for pull
requests only: a push to `main` and a merge-group run each verify a commit
that must keep its own result, while a superseded pull-request run verifies a
commit nobody will merge.

## Owner settings

These are repository settings only the owner can change. The state on
2026-10-01:

| Setting | State |
|---|---|
| `main` ruleset: a pull request, signed commits, and the `conclusion` and `contribution-licence-guard` checks under the strict up-to-date policy; deletion and non-fast-forward pushes blocked; repository administrators may bypass | done |
| `release-tags` ruleset on `refs/tags/v*`: signed tags only, no deletion, no move | done |
| Secret scanning with push protection | done |
| Dependabot alerts and security updates | done |
| Private vulnerability reporting, the channel `SECURITY.md` links | done |
| Immutable releases | done |
| Code scanning in advanced setup, with the CodeQL default setup off so `codeql.yml` is the analysis path | done |
| Actions default `GITHUB_TOKEN` permissions read-only, and workflows cannot approve pull requests | done |
| Auto-merge allowed, squash merges, branches deleted on merge, no merge commits | done |
| The `SONAR_TOKEN` Actions secret, the SonarQube Cloud project `rubentalstra_FerroFED`, and Automatic Analysis off | done |
| The roadmap board ("FerroFED Roadmap") and the label bootstrap (`scripts/gh/labels.sh`) | done |
| Pages publishes from GitHub Actions and serves `ferrofed.eu` with HTTPS enforced; the domain is a Pages setting and a verified account domain, never a `CNAME` file in the tree | pending: lands with the documentation site |
| Registration at bestpractices.dev | done: project [15130](https://www.bestpractices.dev/projects/15130), badge in the README |
| The `github-pages` environment, deploying from `main` only | done |
| A `crates-io` environment with a required reviewer, and one Trusted Publisher entry per crate naming `publish-crates.yml` | pending: the owner's two steps when the `publish` switch is flipped (`docs/release.md` § The crates.io lane) |
| Artifact attestations for the release lane's provenance and SBOM bundles | done: available to a public repository with no setting; the lane writes them from `release-build.yml` and `release-image.yml` |
| The visibility of the `ghcr.io/ferrohealth/ferrofed` package | pending: GHCR creates it private on the first image push; the owner sets it public and links it to the repository |

## Sources

- GitHub Actions security hardening:
  <https://docs.github.com/en/actions/security-for-github-actions/security-hardening-for-github-actions>
- OWASP GitHub Actions Security Cheat Sheet:
  <https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html>
- zizmor: <https://docs.zizmor.sh/>
- actionlint: <https://github.com/rhysd/actionlint>
- hadolint: <https://github.com/hadolint/hadolint>
- Dependabot options reference:
  <https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference>
- Scorecard checks: <https://github.com/ossf/scorecard/blob/main/docs/checks.md>
- Required status checks and rulesets:
  <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- Licensing a repository, and what licensee looks at:
  <https://github.com/licensee/licensee/blob/main/docs/what-we-look-at.md>
