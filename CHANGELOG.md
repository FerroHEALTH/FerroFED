<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintenance rule: every pull request that changes user-visible behaviour adds
an entry under **[Unreleased]** in the same PR. Cutting a release renames
[Unreleased] to the version and date, and adds a fresh link reference.

The architecture is `docs/architecture.md`, the output of the research
program on the v0.0.1 milestone. Releases on the 0.0.x line
start with the repository, its gates and its documentation; the gateway
binary follows from v0.0.2.

## [Unreleased]

### Added

- The static registry (#36): `ferrofed-registry` 0.0.1 loads the federation's
  membership from a reviewed TOML bootstrap document (`[[organisation]]`,
  `[[node]]` with its `system_id` and `[[node.identifier]]`, `[[endpoint]]`)
  with `deny_unknown_fields` throughout into an immutable `RegistrySnapshot`.
  A document with a dangling reference, a duplicate id, a `system_id` shared by
  two nodes (compared without ASCII case), an endpoint without exactly one
  managing organisation, a connection type other than `openehr-rest-query`, an
  unusable base URL or a node without an endpoint refuses to load (N19, N20,
  N21, §12b.2). `NodeId`, `EndpointId` and `SystemId` are distinct types with
  no conversion between them (N32, §12a.1).
- The resolver seam and the development cross-reference (#36):
  `ferrofed-identity` 0.0.1 carries `PatientRef`, the patient identifier
  redacted in every rendering and never serialized (§5.4, N33), the `Resolver`
  trait (N3, §5.2), and `StaticResolver`, a fixed table from a synthetic
  identifier to each member's `ehr_id` that a configuration can enable only
  under `profile = "development"`. It is FerroFED's own testing device, not an
  identity binding.

## [0.0.1] - 2026-10-01

The first release: the repository setup, the documentation site on
<https://ferrofed.eu/>, the architecture of record, the Cargo workspace and the
server binary's shape (configuration, telemetry, health, readiness and
shutdown). The `ferrofed` binary serves health and readiness only; the
federated query follows from v0.0.2.

### Added

- The Cargo workspace (#28): the root manifest with the family lint set, the
  release profile and the `publish = false` switch every library crate
  inherits; the `openehr-*` family declared as one lockstep pin group at 0.0.72,
  with the versions guard failing when one member moves alone; `deny.toml`; the
  `serde_json::Value` ban in `clippy.toml`; and every crate of the architecture's
  crate map as a documented placeholder, with the `ferrofed` binary over a thin
  library run path and the testkit's pin-matrix reader asserting each crate's
  specification constant against `docs/VERSIONS.md`.
- The server binary shape (#29): `ferrofed serve` and `ferrofed config check`;
  configuration from a TOML file and `FERROFED__` environment overrides with
  unknown keys refused, `_file` siblings for every secret, outbound
  credentials per endpoint id, and a typed refusal (exit 78) on any bad value,
  a broken log filter included; `auto`, `json` and `pretty` console formats;
  `GET /`, `GET /health` and `GET /health/readiness` over an indicator
  registry; the request id, the panic catch (a `500` that carries neither the
  panic message nor anything the client sent), the request timeout and the
  body ceiling; a bounded drain on `SIGTERM`; and `501` for every path of the
  ITS-REST surface until the façade lands. The request log records the
  method, the matched route, the status, the latency and the request id, and
  never a body, the AQL text, a header value, an unmatched path, or a query
  value other than the digit-only `offset` and `fetch`, so a façade query's
  patient identifier reaches no log line at any level (§5.4.3).

- The conformance matrix (#41): `conformance/matrix.tsv` with every
  conformance point of section 17, `conformance/tracks.tsv` with the section
  16.3 tracks (track 8 deferred as provisional) and
  `conformance/requirements.tsv` with the reachability of all 46 requirements,
  derived from the vendored specification by `scripts/conformance/matrix.sh`;
  the `// conformance:` test marker; the tier-1 `conformance-matrix` guard; and
  the matrix rendered into the book's Evaluate part.
- The clinical-path storage rule (#40): an engine test reads the crate graph
  and fails when a library crate reaches a storage implementation or the
  application crate.

### Fixed

- `scripts/checks/crate-version-guard.sh` no longer exits early on the change
  that adds the root `Cargo.toml`, where the base has none.

## [0.0.1-rc.1] - 2026-10-01

A pre-release that rehearses the release lane (#14): the repository setup,
the documentation site and the architecture of record, with no binaries.

### Added

- The architecture of record, `docs/architecture.md`, from the first research
  pass (#16, evidence on #18 to #27): the published `openehr-*` crates as the
  openEHR surface at the planned 0.0.74 pin, the request pipeline, the AQL
  rewrite and identifier hygiene on `openehr-query`'s AST, the façade split
  over the ITS-REST route tables, the identity seams with each binding (PIXm,
  mCSD, PMIR, XCPD, the Dutch Generic Functions) in its own crate, the
  security handoff, the registry and its storage, the fan-out, completeness
  and cross-node merge with the `LIMIT` agreement check, the hand-written wire
  types, the crate map with the `publish` switch, the conformance instrument,
  the test topology, the milestone map, and the decision register, every
  entry decided by the owner on 2026-10-01.
- The documentation site on ferrofed.eu: a landing page at `/` and an mdBook
  under `/docs/` organised by reader intent (Evaluate, Operate, Integrate,
  Contribute), built on every pull request and deployed from `main` by
  `docs.yml`, with the pinned docs toolchain, the vendored mermaid assets, the
  favicon set and its `favicon-sync` guard, and the README badge block (#12).
- The release lane, `.github/workflows/release.yml`: a signed `v*` tag is
  checked against `CITATION.cff` and the product row of `docs/VERSIONS.md`,
  the version's `CHANGELOG.md` section becomes the release notes, and the
  release is created as a draft and published once the asset set it promises
  (none before there is code) is verified. `docs/release.md` is the cut (#14).
- The Business Source License 1.1 with Vernum Projecten B.V. as the Licensor
  and copyright holder (#1), and the contribution-licence terms, the
  pull-request checkbox and the `contribution-licence-guard` check (#3).
- The working discipline under `.claude/`: the rules, the hooks (no AI
  attribution, dangerous-command guard, format and comment-style on edit,
  the SessionStart tracker summary), the issue-loop skills, the
  `spec-researcher` and `implementer` agents, and the tracked project memory.
  `CLAUDE.md` states the product, the design-phase status and the hard rules
  (#6), with the identifier-hygiene rule for everything the gateway composes
  for a node.
- The vendored specification corpora under `docs/specs/`, each with a
  `PROVENANCE.md` and a fetch script under `scripts/vendor/`: the Federation
  Tier with AQL specification at v0.9.0 (release candidate, CC0 1.0), its
  reference implementation (Apache-2.0) as evidence, the openEHR ITS-REST
  1.1.0 OpenAPI documents and the openEHR AQL 1.1.0 source with its grammar
  (#7).
- One pin matrix, `docs/VERSIONS.md`, and `scripts/checks/versions.sh`, the
  guard that fails on drift between the matrix and every file repeating a pin
  (#8).
- The GitHub setup: issue and pull-request templates, CODEOWNERS, Dependabot,
  the label set and tracker helpers under `scripts/gh/`, and the CI workflows
  (the tier-1 guards with the Rust tier gated until a workspace exists, one
  `conclusion` check, CodeQL, Scorecard and SonarQube Cloud), and
  `docs/ci-cd.md`, the design of the workflows and the repository settings
  (#9, #10, #11).
- The community and governance set: `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`,
  `SUPPORT.md`, `AI_STATEMENT.md`, `CITATION.cff`, `llms.txt`, and the root
  toolchain, format and lint configuration (#15).

[Unreleased]: https://github.com/rubentalstra/FerroFED/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/rubentalstra/FerroFED/compare/v0.0.1-rc.1...v0.0.1
[0.0.1-rc.1]: https://github.com/rubentalstra/FerroFED/releases/tag/v0.0.1-rc.1
