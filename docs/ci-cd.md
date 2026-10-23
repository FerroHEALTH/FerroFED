<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
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
| `ci.yml` | push to `main`, pull request into `main` or a security patch branch, merge group, dispatch | the two tiers below and the `conclusion` check |
| `contribution-licence.yml` | pull request opened, edited, reopened or synchronized | `contribution-licence-guard`: the pull request body accepts the contribution terms |
| `label-guards.yml` | the `no-changelog` or `no-crate-bump` label added to or removed from a pull request | re-runs the guard that label escapes, and with it `conclusion`, in the CI run on the head commit (§Triggers and concurrency) |
| `codeql.yml` | push to `main` and pull request into `main` or a security patch branch, touching workflows, actions, its configuration or Rust; Mondays | CodeQL in advanced setup; the Actions analysis runs now, the Rust analysis is gated on a root `Cargo.toml` and leaves out the `tests/` trees through `.github/codeql/codeql-config.yml` |
| `scorecard.yml` | push to `main`, a branch-protection change, Mondays | OpenSSF Scorecard, results uploaded to code scanning |
| `sonar.yml` | push to `main`, same-repository pull requests | SonarQube Cloud, advisory; the Rust coverage steps are gated on a root `Cargo.toml` |
| `docs.yml` | push to `main`, pull request, dispatch | builds the site (the landing page at `/`, the book under `/docs/`) on every event and deploys it to GitHub Pages from `main` only |
| `pin-freshness.yml` | Mondays, dispatch | the pins nothing else watches, the testkit images included, compared with upstream, one issue when one is behind; and the acts adopted under Regulation (EU) 2025/327 with the Commission initiatives on it, one issue per newly adopted act |
| `release.yml` | a pushed `v*` tag, dispatch at a tag | the release lane: tag checked against the declared version, changelog section as the notes, draft then publish, with a tag whose tree has no root `Cargo.toml` refused at `plan` (`docs/release.md`) |
| `release-build.yml` | called by `release.yml`, once per target | the SLSA Build Level 3 binary lane: `cargo auditable` build, CycloneDX and syft SBOMs, provenance and SBOM attestations, every asset attached to the draft (`docs/release.md` § The build legs) |
| `release-image.yml` | called by `release.yml` | the container from the attested musl binaries, pushed to `ghcr.io/ferrohealth/ferrofed` by digest with provenance and SBOM attestations as OCI referrers, verified as a consumer would |
| `release-console.yml` | called by `release.yml` | the operator console image: the musl `ferrofed-console` binaries built with `cargo auditable` and the site bundle built with cargo-leptos, each attested, verified, and packed into `ghcr.io/ferrohealth/ferrofed-console` with the same provenance and SBOM attestations as the gateway image (`docs/release.md` § The build legs) |
| `scan-and-tag.yml` | called by `release.yml` | the image gate: both pushed digests scanned on both platforms for HIGH and CRITICAL findings that have a fix (`trivy.yaml`), then the `<version>`, `<major>.<minor>` and `latest` tags applied to the digests that passed |
| `image-scan.yml` | Mondays, dispatch | the published `latest` images, both platforms, scanned with `trivy.yaml` and every OpenVEX document under `security/vex/`, one issue when one carries a finding |
| `publish-crates.yml` | a pushed `v*` tag, dispatch | the crates.io lane behind the workspace `publish` switch: the publishable set from `cargo metadata`, packaged, then uploaded in dependency order through Trusted Publishing; a successful no-op while the switch is `false` (`docs/release.md` § The crates.io lane) |
| `fuzz.yml` | Wednesdays, dispatch, and pull requests touching `crates/openehr-federation`, `app/ferrofed-server`, `fuzz/` or `scripts/fuzz/` | the four `cargo fuzz` targets over the untrusted inputs, time-boxed and advisory, after a check that the generated seeds are current (§The fuzz lane) |

Dependabot (`.github/dependabot.yml`) is the fifteenth piece and is described
under the pins below.

## The two tiers of `ci.yml`

`ci.yml` splits on whether a check needs Rust.

**Tier 1 needs no Rust toolchain**, but for the `manifests` guard, which
reads the manifests with `cargo metadata` and compiles nothing.

| Job | Runs |
|---|---|
| `zizmor` | `zizmor --min-severity=low .github/`, with `GH_TOKEN` so the online audits work |
| `actionlint` | the official image, pinned by tag and digest |
| `shellcheck` | `--severity=style` over every tracked `*.sh` and every tracked extensionless file with a shell shebang, outside `docs/specs/**` and `**/vendor/**` |
| `hadolint` | every tracked Dockerfile under `.hadolint.yaml`, outside the vendored trees, which today is `docker/Dockerfile` and `docker/console/Dockerfile` |
| `kubeconform` | the official image, pinned by tag and digest, in strict mode over the example manifests under `deploy/kubernetes/`, against the schemas of one Kubernetes release at a pinned commit of `yannh/kubernetes-json-schema` |
| `observability` | `scripts/checks/observability.sh --self-test` (a missing tool skips outside CI and fails in it), then `scripts/checks/observability.sh` over `deploy/observability/`, with `promtool` from the Prometheus release tarball, its SHA-256 checked before it runs |
| `comment-style` | `scripts/checks/comment-style.sh --all` |
| `file-length` | `scripts/checks/file-length.sh --self-test`, then `scripts/checks/file-length.sh`: the 1000-line cap on hand-written Rust and on the book's Markdown pages, with its ratchet allow-list |
| `versions` | `scripts/checks/versions.sh --self-test`, then `scripts/checks/versions.sh`: the pin matrix against every file that repeats a pin and each specification row against the crate constant it names, the landing page's release string against the newest `CHANGELOG.md` release, the book's pin table against the rows it names, the vendored provenance stamps and the SPDX licence claims |
| `favicon-sync` | `scripts/checks/favicon-sync.sh`, the book theme favicons byte-identical to the brand favicon set |
| `copyright-holder` | `scripts/checks/copyright-holder.sh`, every first-party `SPDX-FileCopyrightText` header naming Cadasto B.V., the Licensor of `LICENSE`; the vendored trees are skipped |
| `ihe-citations` | `scripts/checks/ihe-citations.sh --self-test`, then `scripts/checks/ihe-citations.sh`: every IHE section the tree cites (`§2:3.83.4.2.2.1`, `ITI TF-1 §27.4.1`, `ITI-72 §3.72.4.3`) is a heading of the vendored text of that profile and version under `docs/specs/ihe-*`, and a profile or transaction named before the `§` matches it; the RESTful ATNA supplement is read with `pdftotext`, a page whose licence keeps it out of the repository (the ITI-55 page, cache only) is read from `.vendor-cache/` when a local run has fetched it, and a transaction no text at hand carries is counted, never checked |
| `bash-compat` | `scripts/checks/bash-compat.sh --self-test`, then `scripts/checks/bash-compat.sh`: every shell program under `scripts/` and the session hooks is free of the constructs bash 3.2, the bash macOS ships, lacks, and expands an array that can be empty only in the guarded form `${a[@]+"${a[@]}"}`, unless it declares the bash it needs with a `BASH_VERSINFO` check |
| `script-modes` | `scripts/checks/script-modes.sh --self-test`, then `scripts/checks/script-modes.sh`, each called by its path: every tracked file under `scripts/` and the session hooks whose first line is a `#!` interpreter line is committed as mode 100755, and every `scripts/<area>/*.sh` program has that line; a sourced library lives under a `lib/` directory |
| `openehr-classes` | `scripts/checks/openehr-classes.sh --self-test`, then `scripts/checks/openehr-classes.sh`: every class an AQL class expression (`CONTAINS COMPOSITION c`), a canonical JSON `_type` or a `CLASS.member` in a comment or a Markdown page names is defined by the vendored RM or BASE class tables under `docs/specs/openehr-rm/` and `docs/specs/openehr-base/`, or titled by the ITS-REST OpenAPI documents, and every member is an attribute, constant or function of that class or a class it inherits; `FROM ENDPOINT` and `FROM ORGANISATION` are the federation directive and are passed over |
| `conformance-matrix` | `scripts/checks/conformance-matrix.sh`, then `scripts/conformance/report.sh --self-test`: the conformance matrix against the vendored specification, the test markers against the matrix, the rendered book page against the matrix, and the README conformance badges under `conformance/badges/` against the matrix and the AQL golden pass list (`docs/architecture.md` section 12) |
| `obligations` | `scripts/checks/obligations.sh --self-test`, then `scripts/checks/obligations.sh`: every row of `conformance/obligations.tsv` with a known status, an issue where its status needs one, an existing test where it is tested, and a point and requirements the matrix holds; the digest of each vendored source's keyword lines against `conformance/obligation-sources.tsv`, so a re-pin fails until the changed sources are reclassified; and the rendered book page against the checklist |
| `technical-documentation` | `scripts/checks/technical-documentation.sh --self-test`, then `scripts/checks/technical-documentation.sh`: every element of Annex III of Regulation (EU) 2025/327, and the Art 37(2) testing-environment reference, has a row in `conformance/technical-documentation.tsv` whose element is a phrase of the vendored Regulation, whose file exists and whose section is a heading of it, with an issue on every partial or planned row, and the book page lists every item; every point of Annex VII of Regulation (EU) 2024/2847 has a row in `conformance/cra-technical-documentation.tsv` held the same way against the vendored CRA, which may also mark a point not-applicable, and the book page lists each as `point <item>`; and when the workspace version's harmonised components (the pin of the published `ehds-logging` crate in the root `Cargo.toml`, and `crates/eehrxf`, `app/ferrofed-eehrxf`, `app/ferrofed-server/src/access`, `app/ferrofed-server/src/fhir`) differ from the newest release tag below it, the clinical safety review table and the book page's change table each have a row for that version (Art 30(2)). It reads the tags from origin and fetches the one it compares with; `release.yml`'s `plan` job runs it too |
| `support-periods` | `scripts/checks/support-periods.sh --self-test`, then `scripts/checks/support-periods.sh`: the support table of `SECURITY.md`, the rows above its marker line, and the table of end dates under point 7 of the book's CRA information to the user (`website/book/src/evaluate/cra-user-information.md`) hold the same releases with the same release and end dates (Regulation (EU) 2024/2847 Art 13(19), Annex II point 7); `release.yml`'s `plan` job runs it too |
| `annex-ii` | `scripts/checks/annex-ii.sh --self-test`, then `scripts/checks/annex-ii.sh`: every point and sub-point of Annex II of Regulation (EU) 2025/327 has one row in `conformance/annex-ii.tsv` whose requirement is a phrase of that point's own text in the vendored Regulation, whose evidence files exist with each named heading of a Markdown file and each named test of a Rust file, with an issue and a note on every partial or planned row and a note on every not-applicable row; the book page lists every point with its status, and the technical documentation index has a point 4 row naming the page and does not mark point 4 documented while an Annex II point is partial or planned |
| `economic-operators` | `scripts/checks/economic-operators.sh --self-test`, then `scripts/checks/economic-operators.sh`: the public copy of the register of economic operators, `docs/registers/economic-operators.tsv` (Regulation (EU) 2025/327 Art 35, Regulation (EU) 2024/2847 Art 23), has the documented column row and no entry, because the rows are kept outside the public repository. The self-test proves each refusal of the row format and of the retention check the owner runs over the private register with `--register FILE`: no row an earlier commit held is removed before its `keep_until` has passed, or changed except by filling that column |
| `e2e-placement` | `scripts/checks/e2e-placement.sh --self-test`, then `scripts/checks/e2e-placement.sh`: every Rust file that checks the `FERROFED_E2E` gate sits in an `e2e` module of its crate's test binary, and the `e2e (containers)` job still sets the gate, selects `test(/^e2e::/)` across the workspace, runs on every change with a workspace (no path filter on the triggers, no job condition beyond `detect`'s) and is one of the jobs `conclusion` needs; every Rust file that checks the `FERROFED_JOURNEYS` gate sits in the testkit's `journeys` module, and the `journeys (browser)` job still sets that gate and selects `test(/^journeys::/)` in the testkit |
| `usage-report-off` | `scripts/checks/usage-report-off.sh --self-test`, then `scripts/checks/usage-report-off.sh`: every shell script and workflow that starts the gateway with `serve` (the binary, a `docker run` of the image, the production guide's `gateway_run`, `docker compose up`) carries `FERROFED__USAGE_REPORT__ENABLED=false`, a `docker run` on its own command line, and every Rust test that starts the binary with `serve` sets it on that `Command`, so no CI run counts as an installation of the usage report (#1122) |
| `aql-splice` | `scripts/checks/aql-splice.sh --self-test`, then `scripts/checks/aql-splice.sh`: no Rust file under `app/`, `crates/` or `tools/` outside a `tests/` or `fixtures/` directory and outside `#[cfg(test)]` builds AQL text by splicing, that is a string literal holding `SELECT` before `FROM`, `FROM EHR`, `CONTAINS`, an `ENDPOINT` or `ORGANISATION` directive, or an AQL path compared with a quoted placeholder, as an argument of `format!`, `format_args!`, `concat!`, `write!`, `writeln!`, `push_str`, `insert_str`, `replace` or `replacen`, or as an operand of `+`; a query is a fixed template parsed by `openehr-query`, its values bound into the syntax tree or sent as ITS-REST `query_parameters`, and printed by `printer::to_aql` |
| `site-links` | `scripts/checks/site-links.sh --self-test`, then `scripts/checks/site-links.sh`: the site assembled by `scripts/site/assemble.sh` with the roadmap block left empty, every internal link and anchor of every page checked by lychee with `--offline --include-fragments`, and `README.md` against the repository tree; `llms.txt` links no file of the coding assistant's working discipline (`CLAUDE.md`, the `.claude/` tree); lychee is the release binary pinned by version and SHA-256, and no request leaves the runner; then `scripts/checks/doc-curl.sh --self-test` and `scripts/checks/doc-curl.sh`: every `curl` command of `README.md`, the book and the landing page sends the body it shows, never a bare number, and a here-document only through `-d @-` |
| `tracker-helpers` | the `--self-test` of `scripts/gh/fields.sh`, `labels.sh`, `migrate-fields.sh` and `rel.sh`, each driven against a stub `gh` on `PATH` |
| `crate-version-guard-self-test` | `scripts/checks/crate-version-guard.sh --self-test`, the guard over a stub repository whose `main` bumped a crate after the branch forked: an untouched crate passes, and packaged content changed without a bump fails |
| `changelog` | `scripts/release/changelog.sh --self-test` and `scripts/checks/changelog-guard.sh --self-test`, each over a stub repository, then `scripts/release/changelog.sh --check`: every fragment under `changelog.d/` is named `<issue>-<kebab-slug>.<section>.md` with a Keep a Changelog 1.1.0 section and holds Markdown list items alone (`changelog.d/README.md`); `scripts/release/plan.sh --self-test` and `scripts/checks/changelog-security.sh --self-test`; then `scripts/checks/changelog-unreleased.sh`, the structure check (Keep a Changelog types and "Upgrade notes", each once per section) and the Security rules (every entry names a `GHSA-` id or `security/vex/` path, a Security section beside other changes opens with its reason) over the fragments assembled under `[Unreleased]` |
| `vex` | `scripts/checks/vex-advisories.sh`: `security/vex/rust-advisories.openvex.json` is exactly what `deny.toml` and `security/vex/rust-advisories.toml` generate, and every OpenVEX document is well formed; then `scripts/checks/vex-coverage.sh --self-test`, the release's vulnerability-record detector (`security/vex/README.md`) |
| `manifests` | `scripts/checks/manifests.sh --self-test`, then `scripts/checks/manifests.sh`: `cargo metadata --locked --format-version 1 --no-deps` over the root `Cargo.toml` and `fuzz/Cargo.toml`, so a manifest that does not parse (a dependency written twice by a merge) fails here with Cargo's own message; the self-test proves a manifest with a key written twice fails. It is the one tier-1 job that installs the pinned toolchain, with no build cache, and `detect` needs it, so a broken manifest starts no Rust job |

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
`Cargo.toml`, the release tool pins in `release-build.yml`,
`release-image.yml` and `release-console.yml`, and the console's build tools
in `ci.yml` and `release-console.yml`, so every comparison runs.

**Tier 2 is gated on the workspace.** A `detect` job checks out and looks
for a root `Cargo.toml`, publishing a boolean output. Every Rust job carries
`needs: detect` and `if: needs.detect.outputs.cargo == 'true'`: rustfmt (after leptosfmt over the console's `view!` bodies, #1155),
clippy at `-D warnings`, nextest plus doctests, rustdoc, `cargo deny check`
(followed by `scripts/checks/advisory-exceptions.sh`, which fails an
advisory exception that outlived its finding, and
`scripts/checks/vex-reachability.sh`, which compares each VEX statement's
dependency path with the graph cargo resolves),
MSRV through `cargo hack check --rust-version`, and `dependency-review` on
pull requests. Each lane mirrors the local command in `.claude/rules/ci-cd.md`.
The lanes were written before the workspace, so the workspace pull request
changed nothing in CI and the lanes activated by themselves; they have run on
every change since.

`e2e (containers)` is the Rust lane that needs Docker. A container-backed test
checks the `FERROFED_E2E` gate first and returns early without it, so the
`test` job stays offline and fast; this job sets `FERROFED_E2E=1` and runs the
container tests (the whole `tools/ferrofed-testkit` suite and the `e2e` module
of every other crate's test binary) against the digest-pinned images of
`docs/VERSIONS.md` §Container images: two FerroEHR instances as the two nodes,
on one FerroEHR PostgreSQL container with a database per node, each behind the
testkit's capturing and fault proxy (`docs/architecture.md` §13). It runs at
nextest's default parallelism, with no test group bounding it: each test
starts one database container for its nodes rather than one per node, and the
suite passes that way on an 8-CPU developer machine (#320). It feeds `conclusion` like every other lane. The tier-1
`e2e-placement` guard keeps every gated test inside an `e2e` module, so the
filter selects it.

The selection rule is that the job runs on every pull request and merge
group of a tree with a workspace, whatever the change touches. A change to
the auth, credentials or configuration code, to an e2e fixture or to the
configuration reference can break a container test while every offline test
passes, so no path filter may ever narrow the lane. The `e2e-placement` guard
refuses a path filter on the `ci.yml` triggers, a job condition other than
`needs.detect.outputs.cargo == 'true'` on the `e2e` job, and a `conclusion`
that does not need it, each with a self-test case (#1187). A pull request
whose `e2e (containers)` job failed or is still running has no green
`conclusion`, so it merges only through the queue after the lane passes. Locally: `FERROFED_E2E=1 cargo nextest run --locked
--workspace -E 'package(ferrofed-testkit) or test(/^e2e::/)'` with Docker
running.

The same job runs the differential run against the Federation Tier
reference implementation (#94), which the testkit builds from the vendored
source at its pinned commit on the digest-pinned Maven and Java images of
`docs/VERSIONS.md`. Each of its two tests writes a Markdown report and a TSV
of differences to `target/differential/`, and the job uploads the directory
as the `differential-report` artifact whatever the tests concluded. A test
fails when it finds a difference its register does not adjudicate, or no
longer finds one the register holds.

`conformance report` writes the report of the specification's section 16.4
(#92). The `ci` nextest profile writes a JUnit report and runs past a
failure, and the `test` and `e2e (containers)` jobs upload theirs as the
`junit-offline` and `junit-gated` artifacts. This job runs whatever those two
concluded, joins both with the `// conformance:` markers through
`scripts/conformance/report.sh`, where a test under `e2e::` counts only from
the gated run, and reports every section 16.3 track and section 17 point as
pass, fail, not-run, deferred or open with the issue and reason of each row
that is not scored. A Node or Operator point is a class of its own (section
16.2): the `e2e (containers)` job also uploads the findings the node profile
checks wrote for each harness CDR product as the `node-profile` artifact, and
the report scores those points from them, never as a gateway pass and never
in its exit status. It adds the report to the job summary,
uploads `target/conformance/` as the `conformance-report` artifact, feeds
`conclusion`, and fails when a covered track or point did not pass. The
`conformance-matrix` job runs the script's `--self-test`. Locally: `bash
scripts/conformance/report.sh --run` with Docker running.

`fhir-validator (EEHRxF documents)` runs HL7's FHIR Validator over every
patient summary document the tests write (#688). The tests that hold a
document to the vendored HL7 Europe Patient Summary profiles also write it
to the directory `FERROFED_EMIT_DIR` names, and do nothing when it is unset;
the `test` job sets it and uploads the directory as the `eehrxf-documents`
artifact. This job runs `scripts/checks/fhir-validator.sh` over that
directory: `validator_cli.jar` at the version `docs/VERSIONS.md` pins,
checked against the SHA-256 its release publishes, on Temurin 21 from
`actions/setup-java`. The validator runs offline, with `-tx n/a` and
`-no-http-access`. It loads `hl7.fhir.eu.eps`, `hl7.fhir.eu.base` and the
MyHealth@EU Master Value Sets Catalogue from their vendored tarballs with
`-ig`, so a value set binding such as `eHDSIAllergenNoDrug` is checked
offline, and reads every package their dependencies name from a package
cache the script fills first: the vendored tarballs, and the packages the
repository does not vendor (the R4 core, HL7 Terminology, the HL7 extension
packages, IPS 1.1.0, IPA, SMART App Launch, DICOM and the validator's own
`hl7.fhir.xver-extensions`), each fetched from the FHIR package registry at
the version a manifest names and checked against the SHA-256 the script
pins. A package it would have to fetch fails the run.

The job fails on an issue of severity error or fatal, on a document with no
outcome, and on a claimed profile the validator could not find, at whatever
severity it reports that. Warnings and information never fail it. Those
include the terminology messages an offline run gives for a code or value
set no loaded package defines, which only a terminology server could check.
The test
documents name their entries under `gateway.example.org`, which RFC 6761
reserves for examples, so the run admits example URLs. The job writes the
counts by document and the warnings by message id to its summary, and
uploads the validator's OperationOutcome output (`validation.json`), its
HTML rendering, its log, `summary.tsv` with every issue, `summary.md` and
the documents as the `eehrxf-fhir-validation` artifact, kept 90 days, which
the technical documentation cites. Its `--self-test` runs first and holds
the judge to those rules over synthetic outputs. It feeds `conclusion`
like every other lane. Locally, with Java 17 or
later: `FERROFED_EMIT_DIR=target/eehrxf-documents cargo nextest run --locked
--all-features -p eehrxf -p ferrofed-eehrxf -p ferrofed-server`, then the
script with `FHIR_VALIDATOR_VERSION` and `FHIR_VALIDATOR_SHA256` set as the
job sets them.

`release compose` holds the compose files every release carries,
`deploy/compose/compose.yaml`, `ferrofed.toml` and `registry.toml`. It builds
`ferrofed` as a static musl binary and runs
`scripts/checks/release-compose.sh` with it: no compose file in the
repository carries `build:`, because every one runs the published image and
only `release-image.yml` builds it; Docker Compose renders the release file;
the stop grace period outlasts the drain delay plus the drain of the example
configuration; and `ferrofed config check` accepts the two examples exactly as attached,
mounted where the compose file mounts them, in the pinned base image of
`docker/Dockerfile` with synthetic credential files, and refuses them once a
member is left out of the PIX Manager. It sits in this tier because the check
needs the binary. Locally, with Docker running, pass a static Linux build of
`ferrofed` for the host's architecture; without one the script runs the
first three checks. The same job runs `scripts/checks/kubernetes-example.sh`
with that binary: it reads `ferrofed.toml` and `registry.toml` out of the
example ConfigMap under `deploy/kubernetes/`, checks that the StatefulSet
mounts the ConfigMap and the `ferrofed-secrets` Secret where the
configuration names them, that each replica keeps its audit spool on a volume
claim of its own, and that the pod's grace period outlasts the drain delay
plus the drain, and runs `ferrofed config check` over it with a
synthetic file for each secret and a synthetic ES384 signing key, then
without the `[signing]` table, which it must refuse. Locally, pass any
`ferrofed` build for the host. It then runs
`scripts/checks/production-guide.sh` with the musl binary: it assembles every
TOML block of the book's production guide into `ferrofed.toml` and
`registry.toml`, holds `deploy/nginx/ferrofed.conf` to their listen address,
base path, JWK Set and PMIR feed routes, runs `ferrofed config check` over
them in the pinned base image, and refuses them without `[audit]
destination`. It then serves them with no network, runs the nginx file in the
gateway's network namespace on the image the `docs/VERSIONS.md` row pins, and
checks each route through the proxy, the `Strict-Transport-Security` header,
and that the proxy's access log names the gateway's request id and never a
query string. Locally, pass a static Linux build for the host's architecture,
as for `release-compose.sh`; without one the script runs its static checks.

`features (cargo-hack)` lints every feature of the published crates, the
server, the identity crate and the engine on its own: `cargo hack clippy
--each-feature --all-targets` over `openehr-federation`, `ihe-iti`,
`nl-generic-functions`, `oauth-server-metadata`, `eehrxf`, `ferrofed-identity`,
`ferrofed-engine` and `ferrofed-server`, at `-D warnings`. Each published crate
is one specification with a feature per layer or profile, and the server, the
identity crate and the engine have one feature per regional binding
(`binding-ihe` and `binding-nl`, which turn on `ihe` and `nl`;
`docs/architecture.md` §6 and §11), so a feature
that only builds beside another one is a defect a caller would hit; the
workspace `clippy` job sees the all-features union only. The job runs per
package, never over the workspace.

`test (ferrofed-server, …)` runs the server's tests in the builds a
deployment can choose that the `test` job never compiles: without default
features, with `binding-ihe` alone, with `binding-nl` alone and with the
default features, each as
`cargo nextest run --package ferrofed-server --no-default-features
--features <build>`. A test that needs a binding is gated on it, or its
fixture names no section of a binding it does not test, so it fails here
when it is neither (#881).

`console (wasm32 and site bundle)` covers the half of the operator console,
`app/ferrofed-console`, that the workspace lanes never compile. The workspace
lanes build, lint and test its server half on the host; this job runs
`cargo clippy -p ferrofed-console --lib --target wasm32-unknown-unknown` at
`-D warnings`, so a dependency that cannot compile for the browser fails here,
and then builds the release site bundle with `scripts/release/console-site.sh
--release`, after `scripts/release/console-tools.sh` has put wasm-opt on
`PATH`, checked against the SHA-256 `docs/VERSIONS.md` pins, so cargo-leptos
never downloads it (the journeys job and the release lane's site job do the
same). The site script refuses to start without it, freezes `Cargo.lock` around cargo-leptos and checks the
WebAssembly, its JavaScript glue, the stylesheet and the favicon are all written.
`scripts/checks/console-bundle.sh` then writes the WebAssembly and JavaScript
sizes, raw and compressed with gzip and brotli, to the job summary, and fails
when the brotli-compressed WebAssembly is over the byte budget the console's
rule file sets, and `scripts/checks/console-paths.sh` fails when the bundle
names a home, runner, registry or toolchain directory of the build host,
which `console-site.sh` remaps away. The console chooses its two halves by compilation target,
never by Cargo feature, so the workspace `--all-features` lanes build it like
any other member and `features (cargo-hack)` has nothing to add for it.

`journeys (browser)` drives the console in a browser (#608). It builds the
same release site bundle, installs Chrome and chromedriver at the Chrome for
Testing release `docs/VERSIONS.md` pins through `browser-actions/setup-chrome`,
names the chromedriver each journey starts on a port of its own, and runs
the testkit's `journeys` module at the profile's full parallelism with
`FERROFED_JOURNEYS=1`: sign-in at a test OpenID Provider, each operator view
with its pagers, a query run that shows every node's status and completeness,
a refused query, and sign-out, each against a running gateway over stub nodes
and failing on any error the browser logs. Without the gate a journey returns
at once, so the `test` and `e2e (containers)` jobs need no browser, and the
`e2e-placement` guard keeps every gated journey where this job's filter
selects it.

`crate-version-guard` runs on pull requests only and fails a change that
alters a `crates/*` member's packaged content without moving its version,
because a published version is immutable (`.claude/rules/crates-publishing.md`).
It reads the changed paths from the merge base of the pull request's base
and head, so a pull request that is behind a `main` that bumped some other
crate passes, and it still requires a changed member to move off the version
`main` holds now. It exits cleanly while no `crates/*` member exists, and the `no-crate-bump`
label is its escape for a diff that provably does not change packaged bytes,
read from the API when the job runs.
Nothing is published yet, behind the workspace `publish` switch
(`.claude/rules/crates-publishing.md`); the guard keeps each crate's line
honest until the switch flips.

`changelog-guard` runs on pull requests only and fails a change that records
no changelog entry: it passes when the change adds a fragment under
`changelog.d/`, or edits `CHANGELOG.md` while pull requests opened before the
fragments still land. Like `crate-version-guard`, it reads the change from the
merge base, so a fragment that reached `main` after the branch forked does not
count. The `no-changelog` label is its escape for a change with no
user-visible effect, and a pull request a bot opened is skipped, as
`contribution-licence-guard` skips it. Both guards read their label from the
API when they run, and adding or removing either label re-runs its guard
through `label-guards.yml`, so a label takes effect without a new push. The
release cut turns
the fragments into the version's section (`docs/release.md`).

**Running the guards locally.** Run with no arguments, every guard under
`scripts/checks/` that needs no built input passes on a clean branch
(#1151). With no arguments, `changelog-guard.sh`
compares `origin/main` with `HEAD` and passes a branch with no commit of its
own, `crate-version-guard.sh` compares `origin/main` with the working tree,
and `stray-literals.sh` reads `app/ferrofed-console/src`, as CI does. Some
guards need an input first, and each one says which when it is missing:
`kubernetes-example.sh` and `upgrade-notes.sh` need a built `ferrofed`
binary, `console-bundle.sh` and `console-paths.sh` need the release site
bundle from `scripts/release/console-site.sh --release`, and
`release-compose.sh` needs Docker, and `fhir-validator.sh` needs the
emitted documents and Java. `pin-freshness.sh` and `ehds-acts.sh` read
upstream sites over the network.

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

A run cancelled as a whole fails `conclusion` too: under `always()` the job
keeps running through the cancellation
([workflow cancellation reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation))
and finds its needed jobs `cancelled`. Nothing inside the workflow tells a
run a newer commit superseded from one cancelled for another reason, so the
check fails closed on every cancellation. A superseded run's failure sits on
the commit it replaced, never on the head of the pull request, because only
a new commit cancels a pull-request run (§Triggers and concurrency).

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
| the zizmor, actionlint, shellcheck, hadolint, kubeconform, lychee, promtool and `validator_cli` versions in `ci.yml` | `pin-freshness.yml`, weekly, and `scripts/checks/versions.sh` against the `docs/VERSIONS.md` rows |
| the FHIR packages the FHIR Validator reads that the repository does not vendor, by version and SHA-256 in `scripts/checks/fhir-validator.sh` | nothing: each is the exact version a vendored manifest, or the validator, names, so a re-pin of either re-derives them |
| the Kubernetes release and the `yannh/kubernetes-json-schema` commit kubeconform validates against | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` rows; a bump is a deliberate change to both |
| the Federation Tier specification and reference implementation commits | `pin-freshness.yml`, weekly, against each repository's `main` |
| the e2e node images, by tag and digest in the testkit's `PinnedImage` constants | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` image rows, and `pin-freshness.yml`, weekly, against the newest stable tag in each image's registry; a bump is a deliberate change to both |
| the EU corpora, `docs/specs/eu-ehds/` and `docs/specs/eu-cra/`, against the acts adopted under, amending or correcting Regulation (EU) 2025/327 and Regulation (EU) 2024/2847 | `pin-freshness.yml`, weekly: `scripts/checks/ehds-acts.sh` reads EUR-Lex and the Have your say register |
| the release and fuzz tool versions (`cargo-auditable`, `cargo-cyclonedx`, `syft`, `cargo-fuzz`) | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` tool rows |
| the Trivy version of the image scans and the vulnerability record, and the `yq` version of the `vex` job | `scripts/checks/versions.sh` against the `docs/VERSIONS.md` rows; a bump is a deliberate change |
| the fuzz seeds generated from the vendored corpora (`fuzz/seeds/*/gen-*`) | `scripts/fuzz/seeds.sh --check`, the first job of `fuzz.yml` |
| `fuzz/Cargo.lock` against the workspace crates the fuzz targets depend on | the `fuzz lockfile` job of `ci.yml`, on every pull request (`cargo metadata --locked`) |
| every pin repeated in a second file | `scripts/checks/versions.sh` against `docs/VERSIONS.md` |

Every Dependabot ecosystem carries a 7-day cooldown. CI is where the
release-signing identity will live, so a compromised action release is the
attack the cooldown buys detection time against; security updates are exempt
from cooldown by design and still arrive at once.

**Dependabot does not read an analyzer version** inside a `run:` block or an
installer input, nor a commit a vendor script fetches, nor an image a
testkit constant names. `pin-freshness.sh` reads those pins from
`docs/VERSIONS.md` and the testkit's `PinnedImage` constants, compares each
with the newest upstream release, commit or stable registry tag, and opens
one issue carrying the report when one is behind, adding nothing when an
open issue already carries it. The issue goes through `scripts/gh/fields.sh new` as a Task at Low priority and Low
effort; the job's default token may not read the organisation's issue types,
and then the issue lands with its `ci` label alone and whoever picks it up
sets the type, the priority and the effort. A pin it
cannot read fails the job, so a network failure never reads as a fresh pin.
It opens an issue rather than failing red, because a weekly red job on a lint
version trains a maintainer to ignore red jobs. For the specification the
issue is the trigger for a re-pin, which is never automatic: a new commit can
change a requirement, and a re-pin re-checks every cited N and CP
(`.claude/rules/spec-adherence.md`).

A testkit image is compared with the newest stable tag of its own shape: a
dotted version with the pinned suffix verbatim, so a pre-release, a variant
such as `-alpine` and a floating tag such as `4.3` never count. Each image is
read from its registry's tag list over the OCI distribution API with an
anonymous pull token. An image whose consumer needs its major line, such as
the Java 21 runtime of the reference implementation, is compared within that
line, and the newest tag of any line is printed beside it. Both
`--self-test` modes run in the `versions` job of `ci.yml`, offline.

The second job, `EHDS and CRA acts`, runs `scripts/checks/ehds-acts.sh`. It
reads from the Publications Office's Cellar, the repository EUR-Lex serves,
every act based on or supplementing, amending or correcting Regulation (EU)
2025/327 or Regulation (EU) 2024/2847, the Cyber Resilience Act, with the
article each one implements. It also reads the Commission's Have your say
initiatives on Regulation (EU) 2025/327 with the stage of the three whose
act the harmonised components wait for (Art 15(1), Art 36(1), Art 40(4)).
An act, amendment or corrigendum that `scripts/vendor/eu.sh` does not pin
and the script does not record as not vendored opens one issue for its
CELEX number, and an initiative that is new or has moved opens one issue
for the register. A source that does not answer fails the job.

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

`ci.yml` runs on `push` to `main`, `pull_request` against `main` or a
security patch branch `release/<major>.<minor>.x`, `merge_group` and
`workflow_dispatch`. A patch branch carries a security patch of the newest
release ([`release.md`](release.md#cutting-a-security-patch-release)); a push to it
runs nothing, because zizmor's cache-poisoning audit reads a push trigger on
a release branch as a release lane, so the pull request's run is its gate.
`label-guards.yml` and `codeql.yml` take the same branches. A pull request's
runs share the concurrency group `ci-<ref>` with `cancel-in-progress`, so a new commit
cancels the run of the commit it replaces, a commit nobody will merge. Every
other run gets a group of its own, `ci-run-<run id>`, so nothing cancels a
push to `main`, a merge-group run or a dispatch, in progress or pending: each
verifies a commit that keeps its own result, and every merge group gets a
real `conclusion`.

A pull request triggers `ci.yml` when it is opened, reopened or gets a new
commit, never on `labeled` or `unlabeled`. A pull request opened with a label
used to start two runs on one commit a second apart, and a concurrency group
orders runs by the time each started waiting, not the time each was created
([concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)).
The run cancelled could be the newer one, and a pull request is held to the
`conclusion` of the most recently created run on its head commit (observed:
GitHub's documentation does not say which of two same-named checks counts).
Its failed `conclusion` then blocked a pull request whose other run had
passed (#788).

The two guards that have a label escape read the pull request's labels from
the API when they run. `label-guards.yml` runs when `no-changelog` or
`no-crate-bump` is added or removed: it waits for the CI run on the head
commit to finish and, when the guard's result no longer matches the labels
(a failure with its escape now set, or a pass with it now removed), re-runs
that job. Re-running a job re-runs its dependent jobs
([re-run a job from a workflow run](https://docs.github.com/en/rest/actions/workflow-runs#re-run-a-job-from-a-workflow-run)),
so `conclusion` reports again in the same run. That workflow decides nothing
a merge depends on: the guard reads the labels itself, and `conclusion`
fails until a re-run passes. A pull request from a fork gets a read-only
token there, so a maintainer re-runs the guard by hand.

Each label reaches the workflows as an event of its own. `gh pr create
--label a --label b` does not attach the labels with the pull request: the
`opened` event comes first, then one `labeled` event per label (observed on
#781, which got five CI runs when `ci.yml` still ran on `labeled`). `ci.yml`
now ignores them, so a label costs no CI run, and `label-guards.yml` starts
one run per label event. For a label other than `no-changelog` or
`no-crate-bump` that run's job is skipped and takes no runner. For one of
those two, the job holds a runner while it waits for the CI run on the head
commit to finish, up to its 60-minute timeout, and may then re-run the guard
and `conclusion`. When a pull request is opened with an escape label and its
guard reads the labels before that label's event lands, the guard fails in
the first run and passes in the re-run.

## Owner settings

These are repository settings only the owner can change. The state on
2026-10-01:

| Setting | State |
|---|---|
| `main` ruleset: a pull request, signed commits, the `conclusion` and `contribution-licence-guard` checks without the strict up-to-date policy (`strict_required_status_checks_policy: false`, read 2026-10-06), and the merge queue (squash, all green, up to five pull requests a group), which tests each pull request on top of `main` and the ones ahead of it, so a branch need not be up to date to merge; deletion and non-fast-forward pushes blocked; repository administrators may bypass | done |
| `release-tags` ruleset on `refs/tags/v*`: signed tags only, no deletion, no move | done |
| Secret scanning with push protection | done |
| Dependabot alerts and security updates | done |
| Private vulnerability reporting, the channel `SECURITY.md` links | done |
| Immutable releases | done |
| Code scanning in advanced setup, with the CodeQL default setup off so `codeql.yml` is the analysis path | done |
| Actions default `GITHUB_TOKEN` permissions read-only, and workflows cannot approve pull requests | done |
| Auto-merge allowed, squash merges, branches deleted on merge, no merge commits | done |
| The `SONAR_TOKEN` Actions secret, the SonarQube Cloud project `FerroHEALTH_FerroFED` in the organization `ferrohealth`, and Automatic Analysis off | done |
| The roadmap board ("FerroFED Roadmap") and the label bootstrap (`scripts/gh/labels.sh`) | done |
| Pages publishes from GitHub Actions and serves `ferrofed.eu` with HTTPS enforced; the domain is a Pages setting and a verified account domain, never a `CNAME` file in the tree | pending: lands with the documentation site |
| Registration at bestpractices.dev | done: project [15130](https://www.bestpractices.dev/projects/15130), badge in the README |
| The `github-pages` environment, deploying from `main` only | done |
| A `crates-io` environment with a required reviewer, and one Trusted Publisher entry per crate naming `publish-crates.yml` | pending: the owner's two steps when the `publish` switch is flipped (`docs/release.md` § The crates.io lane) |
| Artifact attestations for the release lane's provenance and SBOM bundles | done: available to a public repository with no setting; the lane writes them from `release-build.yml`, `release-image.yml` and `release-console.yml` |
| The visibility of the `ghcr.io/ferrohealth/ferrofed` package | pending: GHCR creates it private on the first image push; the owner sets it public and links it to the repository |
| The visibility of the `ghcr.io/ferrohealth/ferrofed-console` package | pending: the same as the gateway's, on the first release that pushes the console image |

## Sources

- GitHub Actions security hardening:
  <https://docs.github.com/en/actions/security-for-github-actions/security-hardening-for-github-actions>
- OWASP GitHub Actions Security Cheat Sheet:
  <https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html>
- zizmor: <https://docs.zizmor.sh/>
- actionlint: <https://github.com/rhysd/actionlint>
- hadolint: <https://github.com/hadolint/hadolint>
- kubeconform: <https://github.com/yannh/kubeconform>
- Dependabot options reference:
  <https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference>
- Scorecard checks: <https://github.com/ossf/scorecard/blob/main/docs/checks.md>
- Required status checks and rulesets:
  <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets>
- Licensing a repository, and what licensee looks at:
  <https://github.com/licensee/licensee/blob/main/docs/what-we-look-at.md>
