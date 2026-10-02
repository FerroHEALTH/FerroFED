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

- The full per-endpoint report (#49; §9.5, §11.1, N16, N40, CP-11, CP-31):
  every registry member appears in `meta.federation.endpoints[]`, an endpoint
  a directed request did not name as `excluded` with the reason, which stays
  out of scope, so it neither clears `complete` nor fails the query. A
  `[[node]]` of the registry document may record its CDR `product` and
  `version`, which the report carries only from there and omits when the
  registry does not say. `latency_ms` appears exactly for the endpoints the
  gateway dispatched to, and `row_count` counts what each node contributed
  before any federation-level `DISTINCT`, dedup or `LIMIT`.
- Completeness (#50; §11.2 to §11.4, N6, N37, CP-30): all-or-nothing stays
  the default, and a request opts into best-effort with
  `openEHR-federation-completeness: partial`. Under best-effort the gateway
  answers `200` with the rows of the nodes that answered, names every other
  node with its status, and sets `complete: false`. A cross-reference that
  cannot answer is reported there and is not a `424`. `all` is accepted
  explicitly. Any other value, a repeated header, or `partial` where
  `federation.best_effort = false` withdraws the mode is a `400` that asks no
  node and never quotes the value. The `not-resolved` and `consent-denied`
  carve-outs and the scope rule hold in both modes. `complete` is always
  derived from the statuses.
- Timeouts (#51; §11.5, N38, CP-31; RFC 7240): a client shortens the budget
  with `Prefer: wait=<seconds>`, and a longer wait leaves the configured
  budget in force. The effective budget is the one reported in
  `meta.federation.timeout`, and a wait that set it is echoed in
  `Preference-Applied`. Only the first `wait` counts; a malformed one is
  ignored and never refused. `wait=0` asks no node and reports each one
  `time-out`. The overall budget runs from the request's arrival, so the
  patient resolution and the fan-out share it and the gateway answers inside
  it, and abandoning one node never aborts a request in flight to another.
- `ORDER BY` with `LIMIT` re-applied at the Tier, with a deterministic
  tie-break (#52; §11.6.1, N9, N13, N39, CP-8, CP-32). Every node is sent the
  client's `LIMIT n` unchanged, with the row's uid appended as the last
  `ORDER BY` key; an `ORDER BY` path the query does not select travels as a
  hidden column the client never sees. The gateway merges the node answers
  under one total order (null greatest, numbers exactly, complete date-times
  by instant, strings by code point, `DV_ORDERED` values through the openEHR
  RM's own comparison), breaks ties on the endpoint id and then the uid, and
  cuts the result at `n`. A node that returned `n` rows out of that order, or
  more than `n`, is reported `node-error`, so the query fails `424` under
  all-or-nothing. `TOP n` is read as `LIMIT n`; `TOP n BACKWARD`, a `TOP` beside a
  `LIMIT` clause, and a `DISTINCT` query ordered on a path it does not
  select are refused `400`. A query with `LIMIT` and no `ORDER BY` now returns
  at most `n` rows across all nodes.
- `OFFSET` paging across a fan-out (#53; §11.6.2, N9, N39, CP-32). `OFFSET`
  never reaches a node. Under the default `federation.offset_strategy =
  "bounded"`, `ORDER BY … LIMIT n OFFSET k` asks each node for `LIMIT k + n`,
  checks each node's visible order as for `LIMIT n`, merges in the federation
  order and returns rows `k` to `k + n`. A page whose `k + n` is past
  `federation.max_offset_window` (1000 rows per node by default; 0 refuses to
  boot), an `OFFSET` with no `LIMIT`, and an `OFFSET` with no `ORDER BY` are
  refused `400`, the first naming the bound. `offset_strategy = "reject"`
  refuses every `OFFSET` past zero `400`. The ITS-REST `offset` and `fetch`
  members follow the same strategy.

### Changed

- The quickstart and the end-to-end harness run two FerroEHR nodes (#155,
  decision A44). EHRbase left both, because it refuses a `.` in
  `PARTY_REF.namespace`, which openEHR BASE admits (#118). In `compose.yaml`
  the services are `ferroehr-a` and `ferroehr-b`, each on its own database
  and with its own `system_id` (`node-a.quickstart.local`,
  `node-b.quickstart.local`, which the quickstart registry declares), and
  node B moved from port 8091 to 8082 (`FERROEHR_B_PORT`; node A's
  `FERROEHR_PORT` is now `FERROEHR_A_PORT`). Every end-to-end case seeds the
  patient's subject on both nodes, and the `e2e (containers)` CI job now runs
  the server's container tests as well as the testkit's.
- The tracker records the kind, the urgency and the size of an issue in
  GitHub's native issue type (Bug, Feature, Task) and the organisation's
  Priority and Effort fields (#154). The `bug`, `enhancement` and `P0` to
  `P3` labels are retired, the bug and feature forms set the issue type, and
  `scripts/gh/fields.sh` and `scripts/gh/migrate-fields.sh` carry the model
  and the migration.

### Fixed

- A query that uses `TOP` together with a `LIMIT` clause is refused `400`
  (`top-with-limit`), whether or not the two counts agree, because AQL
  forbids the pair (#162; AQL §TOP, §LIMIT). A `TOP` query sent with the
  ITS-REST `fetch` member is refused `400` (`top-with-fetch`), because
  ITS-REST says `fetch` "cannot be combined with AQL-top". `TOP n` alone is
  still read as `LIMIT n`.
- A federated query that leaves no registry member in scope, because every
  endpoint is `excluded` (suspended by the operator, for example), answers
  `404` and asks no node, where it answered `200` with empty `rows` (#166;
  §11.1, §11.2, §11.3). A candidate set that localization left empty is not
  that case: every member is `not-localized`, and the answer stays `200`
  with `complete: true` (§14.1). A patient who is `not-resolved` at every
  member in scope still answers `200`.
- A gateway that federates refuses to boot, and `config check` refuses the
  file, unless `server.request_timeout_ms` exceeds
  `federation.overall_timeout_ms` by more than one second, the margin kept
  for combining the answers (#167; §11.5). The refusal names both keys.
  Before, a request timeout between the two let the server's `408`, with no
  body, cut a slow fan-out instead of the `504` that carries
  `meta.federation`.

## [0.0.3] - 2026-10-02

The first two federated milestones in one release (v0.0.2 and v0.0.3; no
v0.0.2 tag was cut). `POST {base}/v1/query/aql` answers one ITS-REST
`RESULT_SET` over two openEHR CDRs, with the patient resolved outside AQL
through a PIXm PIX Manager (or the development cross-reference) on either
patient carrier, and no directly identifying identifier sent to a node: the
rewrite refuses it, and an outbound gate re-checks every request before it
leaves. Also the Cargo workspace and its spec-named crates, the PIXm and PDQm
clients, the container and compose quickstart, the end-to-end harness with a
PIX Manager, the fuzz lane, and the release lane at SLSA Build Level 3,
rehearsed as `v0.0.2-rc.1`.

### Added

- The PDQm ITI-78 patient demographics query client in `ihe-iti`, feature
  `pdqm` (#119; PDQm 3.2.0, ITI TF-2 §2:3.78). `pdqm::PdqmClient` posts a
  `PatientQuery` to `[base]/Patient/_search` as a form body, so no URL carries
  a demographic value, and reads the `searchset` into the matching Patients
  (FHIR R4 `Patient` from `fhir-types` `resources`), the `total`, each match's
  score and `match-grade`, the `OperationOutcome` warnings and the `next` page
  link, which it follows only on the Supplier's origin. A `404` with a
  `not-found` issue for a query that names an identifier domain is the
  profile's unrecognised-domain answer; every other failure is a typed error
  that carries no value, URL or Supplier text. The ITI-78 artefacts of the PDQm
  3.2.0 package are vendored under `docs/specs/ihe-pdqm/` by
  `scripts/vendor/ihe-pdqm.sh`. The `OperationOutcome` issue type moves to
  `ihe_iti::outcome::IssueType`, shared by PIXm and PDQm; `ihe-iti` is 0.0.5.
- The identity-lifecycle hook for track 8 (#48): `ResolutionBindings::identity_changed`
  drops every resolution binding a merge or split at the identity source could
  have made stale, by `ehr_id` in every session or all of them for an
  unscoped change, and `Federation::identity_changed` is the entry point a PMIR
  subscription calls. The binding lifetime, `federation.binding_ttl_ms`, is the
  bound until one exists. Track 8 stays provisional and is not claimed.
- The ask-all node selection of a deployment with no localizer (#46; §4.3
  Variant B, N4, N10). `federation.node_selection = "ask-all"` declares it,
  and a gateway that federates refuses to boot without the declaration
  (`NodeSelectionUndeclared`), so the choice is never a silent default. Every
  active member's cross-reference is asked, the query reaches only the members
  that return an `ehr_id`, and the others are `not-resolved` without failing
  the query (N6, N8). The selection is named in the startup log; the
  `OPTIONS` self-description follows with #73. The quickstart and the
  configuration page declare it.
- A PIX Manager in the test harness, seeded by ITI-104 (#47). The testkit's
  `pix::PixManager` is a test device, not a PIXm implementation: an in-process
  loopback server that takes the PIXm 3.1.0 Patient Identity Feed FHIR
  (ITI-104, a conditional `PUT Patient?identifier=` held to the
  `IHE.PIXm.Patient` minimums, plus the Remove Patient Option's conditional
  `DELETE`) and answers ITI-83 `$ihe-pix` from what was fed, with each case of
  ITI TF-2 §3.83.4.2.2. The seed builder feeds it a synthetic patient in the
  `urn:oid:2.999` arc with one `ehr_id` per node domain (`seed::feed`), and a
  capturing proxy in front of it journals and faults its traffic. The server's
  e2e suite now resolves through it, at one member and at both, while the
  failure classes stay on stubbed Managers. The IG's ITI-104 artefacts (the
  Source `CapabilityStatement`, the `Patient` profiles and the example
  Patients) are added to the vendored PIXm corpus.
- The outbound identifier-hygiene gate and the security events of the
  analysis guard (#45, §5.4, N33, CP-26). Right before a request leaves for a
  node, the engine re-reads its AQL text (raw and as the printer escapes a
  literal), its paging members, its URL (raw and percent-decoded) and the
  headers the gateway adds, against the identifiers resolution consumed, and
  refuses to send a request that still carries one: nothing reaches the node,
  the query fails closed, and the refusal names the part of the request and
  never the value. The gate reads past the `ehr_id` literal the rewrite
  scoped the query to, so a short identifier that occurs inside the node's
  own `ehr_id` is answered. Every patient predicate the rewrite strips, every refused
  query and every gate stop is a security event under `ferrofed::security`,
  located by byte range and carrying no identifier. A clinician or facility
  predicate (composer, care facility, performer, committer) is dispatched
  unchanged; the same path compared with the patient identifier is refused.
  `openehr-federation` 0.0.5 adds `Refusal::kind`, `Refusal::at` and
  `PatientQuery::stripped` for those events.
- The resolution step through a PIX Manager (#43). A `[pixm]` table selects
  the PIXm resolver: each `[[pixm.manager]]` names a PIX Manager's FHIR base,
  its credentials, and the `ehr_id` domain of every member it resolves, and
  `[pixm.namespaces]` maps a client's issuing namespace to a PIX assigning
  authority. One ITI-83 call per Manager resolves the patient at its members;
  only a member that knows the patient is asked its node query, a member that
  does not is `not-resolved` in `meta.federation`, a patient known nowhere is
  a `200` with no rows and `complete: false`, and a Manager that cannot answer
  fails the query `424` with no node asked. The patient identifier and its
  namespace reach the PIX Manager only. Boot is refused when a member has no
  Manager, when a member has two, or when `[dev]` and `[pixm]` are both set.
  The `{node, ehr_id}` set of each resolution is held in memory as the client
  session's resolution bindings, bounded by `federation.binding_ttl_ms`
  (default 15 minutes), and nothing derived from a patient identifier is
  stored or logged.
- The PIXm ITI-83 client (#42): `ihe-iti` 0.0.3, feature `pixm`, asks a PIX
  Manager's `Patient/$ihe-pix` for the identifiers other domains hold for a
  patient, held to the vendored PIXm 3.1.0 `OperationDefinition` and read into
  a cross-reference, the profile's not-found answer, or a typed error (an
  unknown source or target domain, a rejection with its issue types, a
  timeout, a transport failure, a malformed answer). A `404` without a
  `not-found` issue never reads as "patient unknown". Identifier values are
  redacted in `Debug` and carried by no error. The IHE PIXm 3.1.0 package
  artefacts of ITI-83 are vendored under `docs/specs/ihe-pixm/` by
  `scripts/vendor/ihe-pixm.sh`, pinned by version and tarball sha256, and the
  FHIR R4 model comes from `fhir-types` 0.1.107.
- The fuzz lane (#134): four `cargo fuzz` targets over the untrusted inputs
  (the AQL rewrite over arbitrary text and parameters, the ITS-REST
  `AdhocQueryExecute` body through the façade's intake, a federated
  `RESULT_SET` with its `meta.federation`, and the `OPTIONS {base}/` body),
  with seeds generated from the vendored golden cases and the specification's
  JSON examples by `scripts/fuzz/seeds.sh`. The rewrite target asserts the
  identifier-hygiene property of §5.4.1 (N33) on every query it accepts,
  string-function reconstructions included. `fuzz.yml` runs the targets
  weekly, on dispatch and on pull requests touching the code they read.
- Both patient-identifier carriers resolve (#44; §5.4.3, N33, CP-38): an
  `ENTRY`-level `subject` predicate, `…/subject/identifiers/id`, is resolution
  input on equal terms with `EHR_STATUS.subject.external_ref`, with the
  `DV_IDENTIFIER` `issuer` or `type` as its issuing namespace. It is consumed
  and stripped exactly as `external_ref` is, so the same patient query through
  either carrier sends the same node query and returns the same rows. The same
  value in both carriers is consumed once; a second, different value, and
  qualifiers that name two namespaces, are refused with a `400`, as are the
  `assigner`, a predicate on the identifier list, an `ENTRY`-level
  `external_ref`, and the carrier selected, ordered on or inside a function.
  `openehr-federation` is 0.0.4, and the interim `Refusal::EntrySubject` is
  gone.
- The release lane at SLSA Build Level 3 (#31): the reusable
  `release-build.yml` builds `ferrofed` per target with `cargo auditable`,
  writes a CycloneDX and a syft SBOM, and attests the tarball's provenance and
  both SBOMs; the reusable `release-image.yml` builds the `linux/amd64` and
  `linux/arm64` image from the attested musl binaries, pushes it to
  `ghcr.io/ferrohealth/ferrofed` by digest, and attests the index and each
  platform manifest. A release is published only when the draft carries all
  eight assets of every target, and a pre-release is never marked latest.
- The first federated query (#38): `POST {base}/v1/query/aql` answers one
  ITS-REST `RESULT_SET` over every member of the registry, with no federation
  syntax needed (N1, CP-1).
  - The gateway types the `query_parameters`, analyses the query with
    `openehr-federation`'s `aql` rewrite, resolves the patient at every member
    through the configured cross-reference, and sends each member that knows
    the patient standard AQL keyed on its own `ehr_id` (N2, N7, CP-2, CP-4).
  - It fans out under the budget and re-injects the selected subject column
    as the resolution input (N5, CP-7). `meta.federation` names every endpoint
    (N16): a member that does not know the patient is `not-resolved`, a
    suspended endpoint and a second endpoint of one member are `excluded`.
  - A refused query is a `400` ITS-REST error that locates the fault and
    quotes nothing (§5.4.3). Without a cross-reference a patient query fails
    closed with `424` (decision A17), and without a registry the route stays
    `501`.
  - The configuration gains `profile`, `[registry] document`, `[federation]`
    (`per_node_timeout_ms`, `overall_timeout_ms` below the request timeout,
    `default_namespace`) and the `[[dev.crossref]]` rows of the development
    profile. `config check` loads the registry too.
  - The compose quickstart mounts `docker/quickstart/registry.toml` and
    `ferrofed.toml`, and the README shows one federated query returning the
    EHR of both nodes. The end-to-end test runs it against FerroEHR and
    EHRbase behind their capturing proxies, and no node request carries the
    patient identifier in any carrier (N33).
- The fan-out engine, first increment (#37): `ferrofed-engine`'s `fanout`
  module sends one request per in-scope node at once, each under a per-node
  deadline cut to the overall budget, with no retry and no hedging (§11.5,
  N38). A node still outstanding when the overall budget runs out is abandoned
  and reported `time-out`, without touching any other request, and its late
  answer contributes nothing. `meta.federation` is built from every outcome
  before the decision, so a failing answer carries it, with the effective
  `timeout` budget and `complete: false`. Under the all-or-nothing default an
  `offline` or `time-out` node fails the query `504`, a `node-error` fails it
  `424`, `504` taking precedence; `not-resolved` and `consent-denied` fail
  nothing, so a patient found nowhere is a `200` with empty rows (§11.3, §11.4,
  N6, N37). Every endpoint record carries its node, `system_id`, managing
  organisation, base URL and `latency_ms` (§9.5, N40), and the rows of the
  `active` nodes are concatenated in endpoint id order until the merge (#52).
- The crates.io lane behind the workspace `publish` switch (#32):
  `publish-crates.yml` runs on every `v*` tag, reads the publishable set from
  `cargo metadata`, packages it and publishes it in dependency order through
  crates.io Trusted Publishing, and is a successful no-op while the root
  `[workspace.package] publish` is `false`. The `publish-dry-run` job packages
  every library crate on every pull request, and
  `scripts/release/publish-crates.sh` is the shared implementation.
- Node dispatch (#34): `ferrofed-engine`'s `dispatch` module builds one
  `openehr-its` client per registry endpoint, rooted at the endpoint's base URL
  as the registry holds it plus the ITS-REST `v1` segment (N28), and sends each
  node query as the generated `POST {base}/v1/query/aql`, once, with the
  per-node deadline and the gateway's request id. The answer maps to exactly
  one §11.1 status: a result set to `active`, a refused connection or broken
  stream to `offline` with its reason, a passed deadline to `time-out`, and a
  documented error, an undocumented status or a body that is not a result set
  to `node-error` carrying the node's own status and message (§11.2, N16,
  N40). A credential the provider cannot produce, or a request the client
  runtime refuses to compose, is a typed dispatch error, never an endpoint
  status. The engine names no HTTP engine directly, and a test holds it.
- The AQL rewrite, first increment (#35): the `aql` feature of
  `openehr-federation` 0.0.3 parses a façade query with `openehr-query`, binds
  its `query_parameters` before analysis, and consumes the
  `EHR_STATUS.subject.external_ref` patient predicate with its namespace into a
  redacted `Subject` (§5.2). Each node receives the query scoped to its own
  `ehr_id` (§7.1, N7, N29); a selected subject or namespace column is
  re-injected after the merge (N5); and `columns[]` is rendered from the façade
  query alone (N17). The rewrite refuses with a typed `400`, never quoting the
  identifier: a predicate outside the top-level `AND` chain or under another
  operator (§7.1), an integer identifier (decision A6), two different patients
  (A7), an unqualified identifier with no default namespace (A5), a query with
  no patient where a localizer decides the node set (A8), paging members that
  disagree with the query (A10), cross-node `OFFSET` (§11.6.2), an undirected
  aggregate (N14), and the identifier anywhere else in the query (§5.4.1),
  including rebuilt by `CONCAT`, `CONCAT_WS` or `SUBSTRING` over split
  literals, which the rewrite folds before the value test; a string function
  over a literal that cannot be folded is refused on an identifier-bearing path
  (decision A4). The reference implementation's 17 golden cases run as a
  corpus, each adjudicated. CI's test lane runs every feature.
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
- The container image: `docker/Dockerfile` puts the `ferrofed` musl binary on
  distroless static, digest-pinned, as the numeric non-root user `65532`, with
  no shell and a read-only root; `scripts/release/stage-dist.sh` stages the
  binaries of a published release after checking their checksums. `compose.yaml`
  starts the gateway beside two member CDRs, FerroEHR 4.3.1 and EHRbase 2.36.0,
  every image pinned by digest in `docs/VERSIONS.md` and held there by the
  versions guard (#30).
- The test harness in `tools/ferrofed-testkit` (#39): FerroEHR 4.3.1 and
  EHRbase 2.36.0 as two nodes on their own documented databases, pinned by
  digest and started only behind the `FERROFED_E2E` gate; a capturing and
  fault proxy in front of each node, whose journal records every request
  (method, path, query, headers, body) and which refuses, delays or answers
  with a chosen status per node; and a synthetic seed builder that writes
  EHRs, the template and compositions over ITS-REST alone, with patient
  identifiers only inside the `urn:oid:2.999` example arc. CI runs the
  container suite in its own `e2e (containers)` job, and the versions guard
  holds the image pins to `docs/VERSIONS.md`.

### Fixed

- `ihe-iti` 0.0.4 names PMIR's transactions as ITI-93 and ITI-94 in its
  description, feature list and README. ITI-104, which it attributed to PMIR,
  is the PIXm Patient Identity Feed FHIR (#47).

### Changed

- The repository moved to the FerroHEALTH organization,
  <https://github.com/FerroHEALTH/FerroFED>, with the roadmap board as
  <https://github.com/orgs/FerroHEALTH/projects/1>; every link, the image name
  `ghcr.io/ferrohealth/ferrofed` and the crates' `repository` follow it (#123).
- The crate layout names every crate that may be published after the
  specification it implements, one crate per specification with a feature per
  layer or profile (#106): `openehr-federation` 0.0.1 (the Federation Tier
  wire types, formerly `ferrofed-wire`, with the `aql` and `merge` features),
  `ihe-iti` 0.0.1 (features `pixm`, `pdqm`, `mcsd`, `pmir`, `xcpd`) and
  `nl-generic-functions` 0.0.1 (features `nvi`, `mitz`, `lrza`, `nuts-auth`).
  The names are held on crates.io by 0.0.0 placeholders. `ferrofed-registry`,
  `ferrofed-identity` and `ferrofed-engine` move under `app/` and are never
  published. A new CI job lints every feature of the published crates on its
  own, and the architecture test also fails when a binding crate depends on
  FerroFED.
- The `openehr-*` family moves to 0.0.74, the lockstep release with the AST
  visitor, spans, parameter binding and the federation directive in
  `openehr-query`, and the router builder, operation matcher, credentials
  provider and per-call options in `openehr-its` (FerroEHR #3505 to #3514).

### Security

- A configuration that does not parse no longer quotes its source line
  (#133). The TOML reader's error printed the offending line, so a malformed
  `[dev]` row could put its patient identifier, and a mistyped inline secret
  its value, into the `config check` output and the boot failure. The refusal
  now names the file, the line and column, the key path (`dev` alone for any
  line of the `[dev]` table) and the kind of fault, and none of the text.

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
- The federation wire types in `ferrofed-wire` (#33): `meta.federation` with
  `complete` derived from the endpoint statuses (§11.4), the per-endpoint
  record whose shape carries the N40 `error` and `latency_ms` obligations of
  each §11.1 status, the `OPTIONS {base}/` self-description with its two
  schema conditionals (§7a.2), the federation header names, and the one seam
  into the ITS-REST `ResultSetMetadata`, which refuses the flat and
  `_`-prefixed members §9.1 forbids (CP-35). Unknown members of every open
  object round-trip. The tests validate every emitted body against the
  vendored schemas, the specification's §9.4 and §7a.2 examples included,
  fail on drift between the schemas and the types, and pin the rules no
  schema states.

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

[Unreleased]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1...v0.0.3
[0.0.2-rc.1]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1...v0.0.2-rc.1
[0.0.1]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1-rc.1...v0.0.1
[0.0.1-rc.1]: https://github.com/FerroHEALTH/FerroFED/releases/tag/v0.0.1-rc.1
