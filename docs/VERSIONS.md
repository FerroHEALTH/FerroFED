<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Pinned version matrix

This file is the single source of truth for every version pin in FerroFED.
When it and a file that repeats a pin disagree, that is drift. Fix the
disagreement; never let either side silently win.
`scripts/checks/versions.sh` enforces the cross-file agreement it can reach and
skips loudly for the files that do not exist yet, so the guard is useful on a
tree with no Cargo workspace and grows teeth as files appear.

No specification governs this file; it is FerroFED's own design.

## Specifications

The ground for each pin will be the pin table in `docs/architecture.md`, the
output of the v0.0.1 research program, which records why the value is what it
is. Once that file exists, the guard compares the first token of each `Pin`
cell below with the first token of the same row there.

| Item | Pin | Repeated in |
|---|---|---|
| Federation Tier with AQL | 0.9.0 | `docs/architecture.md`, later the `OPTIONS {base}/` self-description (`spec_version`) |
| openEHR ITS-REST | 1.1.0 | `docs/architecture.md`, later the node client and the facade |
| openEHR AQL | 1.1.0 | `docs/architecture.md`, later the AQL rewrite |

The federation specification is a release candidate circulated for comment by
the openEHR Federation Working Group. Its 1.0 release replaces this row and the
two corpus rows below in one change: re-pin, re-run both vendor scripts, and
diff the trees. The pinned commit is past the `0.9.0` git tag: it carries the
SEC review amendments of 2026-09-28 (the `meta.federation` nesting, the
all-or-nothing default, the `node-error` status) while the document still
declares `spec-version: '0.9.0'`.

The federation specification binds ITS-REST by name, Release-1.1.0, so the
ITS-REST row follows it rather than the latest ITS-REST development line.

## Bindings (decided, vendored with their first consumer)

The specification names the IHE ITI profiles as its proposed binding (Annex
A) without naming their versions, and the Dutch Generic Functions IG as a
regional alternative (Annex B), which it names at `fhir.nl.gf#0.3.0`. The
bindings and their versions are decided (`docs/architecture.md` §6, decision
A18): PIXm 3.1.0, mCSD 4.0.0 and PMIR 1.6.0 (CC-BY-4.0) and `fhir.nl.gf`
0.3.0 (EUPL-1.2), each vendored under `docs/specs/` and moved into the corpus
table below by the issue that first reads it (#42, #74 and #86, #48, #87).
XCPD has no FHIR package; its adapter (#85) binds the ITI Technical Framework
revision below, which is not vendored until its terms are read. PDQm is not
used by the gateway. The versions are what the FHIR package registry listed as
latest on 2026-10-01.

| Binding | Package | Latest on 2026-10-01 |
|---|---|---|
| PIXm (ITI-83, ITI-104) | `ihe.iti.pixm` | 3.1.0 |
| PDQm (ITI-78, ITI-119) | `ihe.iti.pdqm` | 3.2.0 |
| PMIR (ITI-93, ITI-94) | `ihe.iti.pmir` | 1.6.0 |
| mCSD | `ihe.iti.mcsd` | 4.0.0 |
| XCPD (ITI-55) | the IHE ITI Technical Framework, no FHIR package | Vol 2 Rev 20.1 (2024-12-12, Final Text) |
| Netherlands Generic Functions | `fhir.nl.gf` | 0.3.0, as Annex B names it |

## Corpora and machine-readable inputs

A corpus is pinned by commit or immutable tag, never by a moving tag or a
`latest` URL, and vendored by a committed `scripts/vendor/*.sh` with a
`PROVENANCE.md` (`.claude/rules/vendored-inputs.md`). Each script below reads
its pin from this table, and `scripts/checks/versions.sh` reads each vendored
`PROVENANCE.md` back and fails when it names a different commit or tag.

| Item | Pin | Repeated in |
|---|---|---|
| Federation Tier with AQL specification | `syntaric/openehr-federation-spec` commit `7162d0c760d23105d62a743bf0ad1073c45fdb85` | `scripts/vendor/federation-spec.sh`, `docs/specs/federation-spec/PROVENANCE.md` |
| Federation Tier reference implementation | `syntaric/openehr-federation-ref` commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5` | `scripts/vendor/federation-ref.sh`, `docs/specs/federation-ref/PROVENANCE.md` |
| openEHR ITS-REST OpenAPI | `openEHR/specifications-ITS-REST` tag `Release-1.1.0`, all seven API modules | `scripts/vendor/its-rest.sh`, `docs/specs/its-rest/PROVENANCE.md` |
| openEHR AQL specification source | `openEHR/specifications-QUERY` tag `Release-1.1.0`, the AQL and AQL examples documents and the grammar | `scripts/vendor/aql.sh`, `docs/specs/aql/PROVENANCE.md` |

## openEHR model crates (crates.io)

The openEHR surface comes from the published `openehr-*` crates, consumed by
version like any other dependency (`docs/architecture.md` §2). FerroEHR
releases them as one lockstep family, so the five rows below are one group:
they move together, and `scripts/checks/versions.sh` fails when one member
moves alone, here or in the root `Cargo.toml` `[workspace.dependencies]`. The
pin is the latest version on crates.io, 0.0.72 on 2026-10-01.

| Item | Pin | Repeated in |
|---|---|---|
| `openehr-query` | 0.0.72 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-its` | 0.0.72 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-base` | 0.0.72 | the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-rm` | 0.0.72 | the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-sdt` | 0.0.72 | the root `Cargo.toml` `[workspace.dependencies]` |

**The planned pin is 0.0.74**, the lockstep release of the whole `openehr-*`
family that carries the federation gaps FerroEHR #3505 to #3513 (the AST
visitor, spans, parameter binding, the `FROM ENDPOINT` directive, the parser
fix, the router builder, the operation matcher with `forward`, the
credentials provider and per-call options; `docs/architecture.md` §2). The
five rows move to 0.0.74 in one change when it is on crates.io. The workspace
root (#28) declares the family at the published release and nothing depends on
it yet; the crates that code against the 0.0.74 APIs (#34, #35) are blocked on
their FerroEHR issues.

## Language and runtime

`rust-toolchain.toml` carries the toolchain, and the root `Cargo.toml` will
carry the edition, the resolver and the MSRV. The release lane builds every
published binary on this toolchain, with no cache.

| Item | Pin | Repeated in |
|---|---|---|
| Rust toolchain | 1.98.1 | `rust-toolchain.toml` `channel` (stable) |
| Edition | 2024 | root `Cargo.toml` `[workspace.package]` `edition` |
| Cargo resolver | 3 | root `Cargo.toml` `[workspace]` `resolver` |
| MSRV | 1.98 | root `Cargo.toml` `[workspace.package]` `rust-version` |

The deliverable is a server binary, so the MSRV tracks the pinned stable
toolchain.

## Databases

A single gateway needs no database (`docs/architecture.md` §8). PostgreSQL is
used only as the optional backend of the stored-query store when several
gateway replicas run. Every PostgreSQL FerroFED itself tests against or
documents is the latest release line, so the image row, added with its first
consumer, pins the latest `postgres:18.x` image by tag and by the digest of its
image index. A member node in the test harness runs its product's documented
database image instead (EHRbase on 16.2, §13).

| Item | Pin | Repeated in |
|---|---|---|
| PostgreSQL | 18 | nothing yet; the image row is added with its first consumer |

## Product and citation version

The product version is the workspace `version` in the root `Cargo.toml`, which
every member inherits. The milestone line is 0.0.x, starting at v0.0.1. The
`v0.0.1-rc.1` pre-release rehearses the release lane (#14); the v0.0.1 release
cut moves this row and every file that repeats it in one pull request.

| Item | Pin | Repeated in |
|---|---|---|
| Product version | 0.0.1-rc.1 | `CITATION.cff` `version`, later the root `Cargo.toml` `[workspace.package]` `version` |

`CITATION.cff` tracks this row exactly, and the guard compares the two whenever
`CITATION.cff` exists. Once the root `Cargo.toml` lands, the guard also compares
its `[workspace.package]` `version` with both.

## Documentation toolchain

The site is an mdBook rendered by `.github/workflows/docs.yml`. Every tool it
installs is pinned here and repeated in the composite action that installs
them, so the book renders the same way in CI as it does on a laptop.

| Item | Pin | Repeated in |
|---|---|---|
| mdBook | 0.5.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-version` |
| mdbook-toc | 0.15.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-toc-version` |
| mdbook-mermaid | 0.17.1 | `.github/actions/docs-toolchain/action.yml` `mdbook-mermaid-version` |

## Licence

| Item | Pin | Repeated in |
|---|---|---|
| Project licence | BUSL-1.1 | `LICENSE`, `NOTICE`, the SPDX header of every first-party file, later the `license` field of every own `Cargo.toml`, the container `image.licenses` label, the README badge |

`LICENSE` names Apache License 2.0 as a licence of its own, as the Change
License four years after each version. `scripts/checks/versions.sh` fails on an
Apache-2.0 or MIT claim in any first-party file.

Third-party and vendored material keeps its upstream terms, recorded beside the
vendored tree (`.claude/rules/vendored-inputs.md`): the federation
specification is CC0-1.0, its reference implementation Apache-2.0, and the
openEHR specifications carry the licences their `PROVENANCE.md` files quote.

## Rust dependency pins

The root `Cargo.toml` `[workspace.dependencies]` table will be the
authoritative, fully pinned third-party crate set. Beyond the openEHR model
crates above, this file does not duplicate crate versions; on any discrepancy
the manifest wins. A crate joins a member with `dep.workspace = true`.

## CI tool pins

The tier-1 lanes of `.github/workflows/ci.yml` run the analyzers below, each
pinned to an exact version so a CI result matches the local one. `zizmor` and
`shellcheck` are fetched by `taiki-e/install-action`, which verifies the
upstream release checksum; `actionlint` and `hadolint` run from their official
container images, pinned by tag and by digest.

| Item | Pin | Repeated in |
|---|---|---|
| `zizmor` | 1.30.1 | `.github/workflows/ci.yml` |
| `actionlint` | 1.7.12 | `.github/workflows/ci.yml` |
| `shellcheck` | 0.11.0 | `.github/workflows/ci.yml` |
| `hadolint` | 2.15.1 | `.github/workflows/ci.yml` |

Keep the locally installed versions on these numbers, so a finding costs a
local run rather than a CI round trip (`.claude/rules/ci-cd.md`).

## Release tool pins

The release lane builds and describes every published artifact with the tools
below, each fetched by the digest-pinned `taiki-e/install-action`, which
verifies the upstream release checksum. They decide what a consumer can prove
about a binary, so a floating version here would change the contents of a
release without a reviewed change.

| Item | Pin | Repeated in |
|---|---|---|
| `cargo-auditable` | 0.7.5 | `.github/workflows/release-build.yml` |
| `cargo-cyclonedx` | 0.5.9 | `.github/workflows/release-build.yml` |
| `syft` | 1.51.1 | `.github/workflows/release-build.yml`, `.github/workflows/release-image.yml` |

`scripts/checks/versions.sh` reads every `tool:` line of the release workflows
back against these rows, so a bump moves one row and the workflows follow it.

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`). Dependabot bumps them,
and zizmor checks the form.
