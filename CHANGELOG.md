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
program on the v0.0.1 milestone. Releases on the 0.0.x line started with the
repository, its gates, its documentation and the server shape in 0.0.1; the
federated query and identity resolution shipped in 0.0.3.

## [Unreleased]

### Added

- The registry maps every observed `creating_system_id` (#67; §12.2, N21,
  the mapping half of CP-13): a `[[creating_system]]` entry in the registry
  document maps a `creating_system_id` that is no member's own `system_id` to
  an endpoint, and a member's own `system_id` maps to that member without one.
  The document is refused, by `config check` too and naming the
  `creating_system_id`, when a mapping names an undeclared endpoint, maps one
  id twice (ASCII case aside), or maps a member's own `system_id`.
  `ferrofed-registry` adds the learned map: the first sighting of an id the
  document does not route learns a read route to the endpoint it was seen
  at, and a learned mapping never overrides the document. A sighting at a
  second node, or a learned mapping the document contradicts, withdraws it
  and raises an integrity incident, logged at `ERROR` with a stable kind and
  routing ids only. An id nothing routes is a typed miss, never a default
  endpoint. The follow-up read routing that consumes the table is #64.

- Opt-in version-identity dedup (#56; §10, N15, N36, CP-9, CP-29): a request
  that sends `openEHR-federation-dedup: version-identity` gets one copy of a
  version held at several endpoints, keyed on the full `OBJECT_VERSION_ID`
  of the row's `COMPOSITION`, else `VERSION`, uid as `openehr-base` reads
  it. The copy kept is the one from the endpoint whose registry `system_id`
  is the version's `creating_system_id`, else the one from the lowest
  endpoint id. Two versions of one object are two rows, and a row with no
  version uid is never suppressed. `meta.federation.dedup` records the mode
  on every answer, `none` and failing `424` and `504` envelopes included,
  and beside the rows `suppressed_rows` and `suppressed_endpoints[]`, counted
  before `DISTINCT`, `OFFSET` and `LIMIT`. Every node is asked the version
  uid, as a hidden column when the client does not select it (under
  `DISTINCT` only a selected uid is the key), and a query with a `LIMIT` and
  no `ORDER BY` is ordered on it. Under the mode a tie on the `ORDER BY`
  keys breaks on the uid before `endpoint_id`, which keeps a per-node
  `LIMIT n` (or `k + n` for a page) exact after suppression (§11.6.1). The
  default stays `none` (§10.1), which `none` states explicitly; any other
  value, or a repeated header, is `400` `dedup-invalid`. A node whose version
  uid is not an `OBJECT_VERSION_ID` is `node-error`, and the query fails
  `424` under all-or-nothing. A recombined aggregate under the mode is `400`
  `indecomposable-aggregate` (§11.6.3). `openehr-federation` 0.0.20 adds
  `dedup::DedupMode`, `aql::Context::with_dedup`,
  `order::ResultOrder::with_version_key`, `merge::NodeAnswer::with_system_id`,
  `merge::Suppressed`, `merge::Disagreement::VersionId` and
  `aql::refusal::Indecomposable::Dedup`.

- `SELECT DISTINCT` at the Tier (#55; N13, CP-8, CP-32): a row two nodes
  return is answered once, compared on the columns the client selected under
  the Tier comparator (`2` and `2.0` are one value, two spellings of one
  instant are two), and the duplicates are removed before the `LIMIT` and
  the `OFFSET` (AQL 1.1.0 §LIMIT), so a duplicate no longer takes two slots
  of a `LIMIT n` answer or of a bounded `OFFSET` page. The copy kept is the
  first under `ORDER BY`, then `endpoint_id`. A re-injected subject column
  and a column the gateway adds never make two rows distinct. Each
  endpoint's `row_count` stays what it contributed (§9.5). A node that
  returned its full `LIMIT` with two rows the Tier holds equal is
  `node-error`, because a distinct row can lie past its cut, and the query
  fails `424` under all-or-nothing. `openehr-federation` 0.0.18 adds
  `order::ResultOrder::with_distinct` and `distinct`, and
  `merge::Disagreement::Distinct`.

- Decomposable aggregates (#54; §11.6.3, N14, N39, CP-10, CP-32): an
  undirected `COUNT`, `SUM`, `MIN`, `MAX` or `AVG` is sent to every node and
  answered with one recombined row in the client's columns, never one row
  per node. Counts and sums add exactly, reals in decimal arithmetic; `MIN`
  and `MAX` are re-applied over numbers and complete date-times; `AVG` is
  asked of each node as its `SUM` and `COUNT`. A node value the
  recombination cannot use is `node-error`, so the query fails `424` with no
  value. `DISTINCT`, `COUNT(DISTINCT …)` and a plain column beside an
  aggregate are refused `400` (`indecomposable-aggregate`), and a `partial`
  request for a recombined aggregate is refused `400` (`partial-aggregate`).
  The new `[federation] decomposable_aggregates` list (all five by default,
  `[]` for none) declares the functions, and an undeclared one is still
  `undirected-aggregate`. A directed single-node aggregate is sent unchanged.
  `openehr-federation` 0.0.13 adds the `aggregate` module,
  `aql::Context::with_decomposable_aggregates` and `merge::combine`, and
  takes `rust_decimal` 1.43.0, already in the tree through `openehr-rm`.

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
- The `comment-style` guard checks citations (#173). It fails on a Rust
  comment, doc comment or lint `reason` that cites `docs/architecture.md`, a
  path under `.claude/` or a rule file by name, or that names a
  decision-register entry such as `decision A17`. It applies the same check to
  the full-line `#` comments of the shell scripts under `scripts/` and of every
  `Cargo.toml`. `comment-style.sh --self-test` proves each refused form and
  its near misses, and CI runs it before the full-tree pass. Every comment that
  cited one of these now cites the specification section it rests on, or says
  that no specification governs it.
- The error vocabulary (#57; §11.2, N32, N36, N37, N42, CP-12, CP-30): every
  failure the gateway reports on its own behalf answers the ITS-REST `Error`
  body with a stable `code` and the `request_id`, and its status follows one
  table. A refused query is a `400` named by its refusal (`not-aql`,
  `unreducible`, `offset-unsupported`, and the rest), an unknown path a `404`
  `not-found`, an unexposed ITS-REST area a `501` `not-implemented`, and the
  gateway's own fault a `500` `internal`, and a request with no member in
  scope a `404` `no-destination`; `ehr-id-collision` and
  `controlling-system-unreachable` (`409`) are fixed for follow-up routing.
  The codes are API and are only ever added. The book's "Errors and status
  codes" page lists them all, and a test holds the page to the gateway's
  table.
- The `versions` guard holds two hand-typed version facts to their sources
  (#181). The landing page's release note and status panel must name the
  newest `## [x.y.z]` release of `CHANGELOG.md`, so a release cut that forgets
  the page fails. Each specification row of `docs/VERSIONS.md` must name a
  crate constant (`FEDERATION_SPEC`, `ITS_REST`, `AQL`) that carries the
  version the row pins. `versions.sh --self-test` proves both checks, and CI
  runs it before the full pass.
- Generated conformance badges on the README (#175), as shields.io endpoint
  files under `conformance/badges/`: the Gateway points of §17 covered out of
  the Gateway total, the Node and Operator point counts, each labelled with the
  pinned specification version and linked to the book's conformance page, and
  the AQL golden cases passed out of the vendored corpus, linked to its pass
  list. `scripts/conformance/matrix.sh --badges-write` writes the files and the
  README block between `conformance:begin` and `conformance:end`, and the
  `conformance-matrix` guard fails when either drifts from the matrix or the
  pass list. The golden test fails when a case in
  `conformance/aql-golden/pass-list.txt` stops passing or an unlisted case
  passes, and rewrites the list when `FERROFED_CONFORMANCE_UPDATE` is `1`.
  Case 02 (`FROM ENDPOINT`) is refused until #70 and is not counted as
  passing. The static badge row gains the image-pulls badge for
  `ghcr.io/ferrohealth/ferrofed`.
- The `versions` guard holds the book's "Pinned versions" page to
  `docs/VERSIONS.md` (#198). Each row of its pin table names its matrix rows
  by their exact names, and every pin it states, a version, a package, an
  `edition 2024` or an abbreviated `commit`, must be the one those rows pin;
  a pin the guard cannot read fails rather than passing unread. The page now
  lists all five `openehr-*` crates, the vendored PIXm and PDQm packages and
  the specification's source commit as rows of their own. `versions.sh
  --self-test` proves each drift.
- Tests that pin the two optional facilities FerroFED does not offer (#59,
  #60; §11.6.4, §11.7, CP-31, CP-32). A query sent with `Prefer:
  respond-async`, alone or beside `Prefer: wait`, gets the ordinary
  synchronous answer under the same budget: never a `202` or a
  `Content-Location`, and `Preference-Applied` never names `respond-async`
  (RFC 7240 §2, §3). A bounded `OFFSET` page carries no cursor handle or
  expiry in `meta.federation` and runs the fan-out on every request.

### Changed

- Incompleteness is carried by `meta.federation.complete` alone, and the
  gateway emits no FHIR `OperationOutcome` (#58; §9.1, §11.4, N17, CP-12).
  §11.4, CP-12 and track 4 ask for the resource only for a FHIR-facing
  consumer, and the answer is an ITS-REST `RESULT_SET` whose federation
  additions live under `meta.federation`. The conformance matrix gives CP-12
  that reason beside its scored status codes, a test asserts that a
  best-effort `200` and an all-or-nothing `424` each carry `complete: false`,
  validate against `federated-result-set.schema.json` and hold no
  `OperationOutcome`, and the client contract in the book says so. Where the
  resource would travel for a gateway with FHIR-facing consumers is recorded
  as an upstream report (#204).
- The conformance record's loose ends (#196). The `comment-style` citation
  checks read the conformance tables under `conformance/`: their `#` comment
  lines and their `reason` column, which the book renders. The tables now
  cite §16.2, §16.3, §16.4 and §17 where they cited an internal file or a
  decision-register entry, or say that no specification governs the choice.
  The CP-10 and CP-32 rows name #187. The book page links the specification
  site at the version `docs/VERSIONS.md` pins, with no second copy of it in
  `scripts/conformance/matrix.sh`. `openehr-federation` 0.0.16 marks its
  local list of AQL function names for removal (#195).
- The `comment-style` citation checks cover the rest of the tree (#178): the
  full-line comments of the workflow and composite-action YAML under
  `.github/`, the `echo` and `printf` text of a workflow `run:` block,
  `clippy.toml` with its `reason` strings, and the quickstart TOML under
  `docker/`. The per-edit hook runs the guard on every file kind CI checks.
  Every comment, lint reason and printed line there that cited an internal
  file or a decision-register entry now cites the specification section or
  official documentation it rests on, or says that no specification governs
  it. The six vendor scripts no longer write an internal path into their
  `PROVENANCE.md`.
- The `comment-style` citation checks refuse a citation of any markdown file
  under `docs/` outside the vendored `docs/specs/` tree (#184): a path that
  opens a parenthetical or is followed by `section` or `§`. Naming
  `docs/VERSIONS.md` as the file a script or test reads still passes. The
  checks also read every YAML `description:` scalar and the trailing `#`
  comment after a YAML or TOML value, outside quoted strings and block
  scalars, and the guard runs the same under mawk. The citations this
  catches cite the GitHub documentation or say that no specification
  governs them, among them `fuzz.yml`, the `setup-rust` action
  description, the version and corpus scripts and the CI rule. The crate
  manifests' comment on the 0.0.0 name reservation changes with them, so
  `openehr-federation` moves to 0.0.15, `ihe-iti` to 0.0.7 and
  `nl-generic-functions` to 0.0.4 with no change to their code.
- The `comment-style` citation checks also refuse a citation of a
  `README.md` of the tree, at the root or in a member, outside the vendored
  `docs/specs/` and `vendor/` trees (#198), in the same citation form. Naming
  a README as a file a script reads still passes. `fuzz.yml` cites the
  libFuzzer documentation for how a crashing input is kept, and the scripts
  that cited a README no longer do.
- The release lane fails a tag whose tree has no root `Cargo.toml` (#198),
  with a message saying the tag cannot be checked against the workspace
  version. It used to skip that check.
- The release lane has one path (#201): the binaries and the image build for
  every tag `plan` accepts, and `finalize-release` publishes only when every
  build leg succeeded. The branches for a tag with no workspace, which `plan`
  now refuses, are gone. The book's claims page links to the pinned-versions
  page for the `openehr-*` pin instead of restating the number, and
  `docs/ci-cd.md` names all four self-tests of the `tracker-helpers` job,
  `rel.sh` among them.
- `scripts/gh/fields.sh` checks its arguments before any `gh` call (#198).
  No argument, an unknown command, a wrong operand count or `--help` prints
  the usage to stderr and exits 2, with no network call and no token needed.
  `fields.sh --self-test` proves it against a stub that records every call.
- The per-edit `comment-style` hook also reads the `conformance/*.tsv`
  tables (#198), as CI does, and stays a quiet pass for any other file.
  Conformance track 5 (Dedup + DISTINCT) names #187 beside its issues, as its
  CP-10 and CP-32 rows do, and the book's conformance page is re-rendered.
- Error bodies (#57): the gateway's own refusals (`404`, `501`, a caught
  panic's `500`) answer the ITS-REST `Error` shape with `code` and
  `request_id`, where they named the code in an `error` member, and the codes
  are kebab-case (`not-found`, `not-implemented`). No error body quotes the
  query, a parameter value or a header value (§5.4.3); a `424` or `504` under
  all-or-nothing stays the §11.4 result set and echoes the client's own `q`
  (N17). A node row shorter than the dispatched query selects is now that
  node's `node-error` (`424`, or reported under `partial`), where the whole
  query answered `502` (§11.1, §11.2).
- The `openehr-*` family moves from 0.0.74 to 0.0.76 (#57). 0.0.76 keeps the
  members an open ITS-REST schema admits in an `additional_properties` map
  (FerroEHR #3526), so every error body is the generated ITS-REST `Error`
  with `code` and `request_id` in that map, and no FerroFED-side error type
  remains. The generated request and result-set types carry the same map.
- The site, the README and the book describe the released gateway (#176).
  The landing page opens with the FerroFED mark and the current release, and
  shows the quickstart, the release binaries and the image
  `ghcr.io/ferrohealth/ferrofed`, with what each specification property has
  in v0.0.3 and what is planned. The README's install section names the
  release assets and their verification, and its quickstart runs the
  published image with `docker compose up --wait`. The book's introduction,
  claims, deployment, container and contribution pages, the governance and
  contribution guides, and the working rules no longer describe a design
  phase or a project with nothing to install.
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

- A `SELECT DISTINCT` query with `ORDER BY` and `LIMIT` (or a bounded
  `OFFSET` page) whose selected function column the selected paths do not
  fix is refused `400` (`unordered-distinct-cut`), where a node could cut
  among distinct rows tied on every path differently on each repeat (#210;
  §11.6.1, CP-8, CP-32). AQL orders only on paths (AQL master03-syntax
  §ORDER BY), and under `DISTINCT` the gateway adds no column, so the
  selected paths are a node's only keys. A call to a single-row function AQL
  defines whose arguments are literals, parameters and selected paths, such
  as `LENGTH(c/name/value)` beside `c/name/value`, is answered as before; a
  call that reads a path the query does not select, a clock function such as
  `NOW()`, and `TERMINOLOGY` are refused. The same query without a `LIMIT`
  is answered. `openehr-federation` 0.0.22 adds
  `Refusal::UnorderedDistinctCut`.
- Version-identity dedup compares identifiers without regard to case (#225;
  §10.2, CP-9; BASE `master05-identification_package.adoc` §"Composite
  Identifiers and Case"). Two copies whose version ids differ only in case
  are one version, and the node whose `system_id` differs from the version's
  `creating_system_id` only in case keeps the originating copy. Before, both
  were compared byte for byte. A kept row still comes back as its node sent
  it. The comparison is `openehr-base`'s `composite_ids_equal` and
  `composite_id_key`, which the registry identifiers already use, and the
  session-scoped resolution bindings now key on `EhrId` in place of their
  own case fold. `openehr-federation` is 0.0.21.
- A patient query is no longer refused at random when its withheld
  identifier is a short hexadecimal value that occurs inside the minted
  `X-Request-Id` (#227; §5.4.1, N33, CP-26). The outbound gate reads the AQL
  text, the paging, the URL and every other header the gateway adds, and
  skips the minted id, which `OutboundId::mint` makes from no client input.
  A request a panic, the request timeout or the body ceiling answers now has
  its request-log line, with the status it answered (`500`, `408`, `413`)
  under the gateway's id. The "the federated query failed" line carries the
  same `request_id` as the request line. The hygiene assertions of the tests
  compare the raw bytes a mock node received, so a header value or a body
  that is not UTF-8 is searched too, and the CP-26 row of the conformance
  matrix names #217.
- CP-29 is `planned` again in the conformance matrix and the gateway badge
  (#221): only its visibility half, the dedup record of §10.2 and §10.3, is
  built, and its write-routing and `409` half is #66. The dedup tests carry
  CP-9 alone. `scripts/checks/crate-version-guard.sh` run without its base
  ref prints its usage on stderr and exits 2.
- The book's "What FerroFED claims" page lists decomposable aggregates (#54)
  among what has merged since v0.0.3, and no longer as planned; de-duplication
  is what remains planned for v0.0.4 (#211). The `merge` module doc of
  `openehr-federation` is wrapped like the rest of the crate's docs, and
  `openehr-federation` moves to 0.0.19 with no change to its code.
- An `ORDER BY` with `LIMIT` query whose rows carry no uid, such as one that
  selects from `EHR` alone, returns the same rows on every repeat (#157;
  §11.6.1, §11.6.2, N39, CP-32). Each node is asked to order on a row key
  after the client's keys: the uid of the first `COMPOSITION` or `VERSION`
  as before, else `<ehr>/ehr_id/value`, else the uid of an `EHR_STATUS` or
  `EHR_ACCESS`; the key travels as a hidden column and never reaches
  `columns[]`. A node cut at `LIMIT n`, or at the `LIMIT k + n` of a bounded
  `OFFSET` page, then keeps the same tied rows each time, so two pages agree
  on the rows tied across their edge. A patient query, scoped to one
  `ehr_id` per node, gets no `EHR` key; `FOLDER` is never a key; a
  `DISTINCT` query keeps its selected columns as the tie-break and gains no
  column. A query with no row key is still answered: the Tier orders the
  rows it receives, and which tied rows a node returns is the node's.
  `openehr-federation` 0.0.17 carries the change.
- A query that calls a function AQL 1.1.0 does not define, such as a
  product-specific `MEDIAN(x)`, in `SELECT` or `WHERE` is refused `400`
  (`undefined-function`) when it would reach more than one node, where it was
  sent to every node and answered one row per node (#187; §11.6.3, N14, N39,
  CP-10, CP-32). The gateway cannot tell whether such a function aggregates,
  and per-node aggregate rows are the answer §11.6.3 forbids. Directed to one
  endpoint, the query is sent unchanged. The single-row functions AQL defines
  (AQL §Functions: the string, numeric, and date and time functions, and
  `TERMINOLOGY`) are still sent to every node as written, and the five
  aggregates keep the decomposition rules. `openehr-federation` 0.0.14 adds
  `aql::refusal::Refusal::UndefinedFunction`.
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
- The documentation describes the gateway that exists (#163). The
  `openehr-federation` README names both subject carriers as resolution
  input. The book's client contract and configuration pages name
  `POST /v1/query/aql` as served, and the `_file` secrets paragraph sits
  under the configuration file it describes. Code and doc comments cite the
  specification sections a decision rests on, or say that no specification
  governs it.
- The landing page no longer scrolls sideways on a phone narrower than about
  380 px (#182). The audience cards and the other card grids take a column
  minimum that shrinks to the page width, so a 320 px viewport shows one
  full-width column in the light and dark themes.
- `llms.txt` describes follow-up routing to the owning CDR as planned for
  v0.0.5, where it described it as built (#180). The CI log, the guard
  scripts, the analyzer configuration and the `research` label no longer
  describe the repository as being in a design phase.
- `scripts/gh/labels.sh` prints its usage and exits `2` on any argument it
  does not know, `--help` included, before it calls `gh` (#180). It ignored
  such an argument and wrote the label taxonomy to the repository. Its
  `--self-test` proves the refusal.
- The `ferrofed-engine` crate description names what it holds (dispatch,
  fan-out, the budgets, the completeness decision and the outbound
  identifier-hygiene gate) and lists follow-up routing on the creating
  system id as planned, where it described routing as built (#191). The
  workflows, the release checklist, the version pages of the book and the
  repository, and the working rules no longer describe the Cargo workspace
  as not yet existing, and the book names the `openehr-*` pin as 0.0.76 and
  the PIXm and PDQm packages as vendored.
- `scripts/gh/rel.sh` prints its usage and exits `2` before it calls `gh` on
  a usage error: no argument, an unknown command, a wrong operand count, a
  flag other than `--replace`, or `--help` (#191). It exited `1` with no
  argument, `0` on `--help`, and resolved the repository through `gh`
  first. Its new `--self-test`, run by the CI `tracker-helpers` job, proves
  each write's endpoint and each refusal.

### Security

- A client's `X-Request-Id` no longer reaches any node (#217; §5.4.1, N33,
  CP-26). A legal client value was sent to every node of the fan-out as it
  came, so a patient identifier written into it passed the outbound gate,
  which checks only the identifiers the gateway resolved on. The gateway now
  mints its own id, a version 4 UUID, for every request and sends only that,
  the same id to every node of one request. The response header and the
  error bodies still name the client's own id. The request log, the security
  events and the panic line record the gateway's id and never the client's,
  with a `client_named` flag on the request line saying whether the client
  sent one; a client that sends none gets the gateway's id back. The book
  lists every header a node request carries and where its value comes from.
  `ferrofed-engine` adds `outbound_id::OutboundId`, which only
  `OutboundId::mint` makes, and `DispatchOptions::with_request_id` and
  `fanout::fan_out` take one in place of a string.

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
