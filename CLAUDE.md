<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md

**FerroFED** is a pure-Rust openEHR federation gateway: a transparent
ITS-REST intermediary in front of many openEHR CDRs. A client sends it an
ordinary ITS-REST request, with AQL that carries no federation syntax, and
gets an ordinary ITS-REST answer back, whose `meta.federation` names each
member node with its status. The gateway resolves the
patient outside AQL, through an identifier cross-reference service, to a set of
`{node, local ehr_id}`; it sends standard, non-federated AQL to each node,
scoped to that node's own `ehr_id`; and it merges what comes back with each
node's provenance. It routes follow-up reads and writes to the CDR that owns
them (§12). No directly identifying patient identifier
travels in a dispatched query. It holds no clinical data of its own. It implements the openEHR Federation Working
Group's *Proposal for Federation Tier with AQL* (the Federation Tier
specification), and it reaches every node over the openEHR ITS-REST API.

The name follows the Ferro family (FerroEHR, FerroTERM, FerroBRIDGE, and the
rest of FerroHEALTH). FerroFED in prose, `ferrofed` in identifiers.

## Status: building v0.0.23 and v0.1.0

v0.0.22 is the newest release (2026-11-27); the open milestones are v0.0.23
and v0.1.0. What each release shipped is its section of `CHANGELOG.md`, and
the milestones on the tracker hold what is being built. The gateway serves
the Federation Tier on its ITS-REST face (the federated query and its
merge, follow-up routing, definitions and stored queries, the identity
bindings of Annex A and Annex B) and, as an EHR system under Regulation
(EU) 2025/327, the European exchange format on its FHIR face: the patient
summary, the laboratory, discharge and imaging reports, prescriptions and
dispensations, and a national contact point's connector.

The design of record is `docs/architecture.md`, the output of the first
research pass on #16 (the
evidence is on #18 to #27), with every decision in its register decided by the
owner (2026-10-01, and A43 and A44 on 2026-10-02): how the AQL rewrite and the identifier-hygiene gate sit on
the published `openehr-query`, the identity seams and bindings, the registry
and its storage, the merge across nodes, the wire types, the crate map, and
the conformance instrument. Read it before proposing anything structural.
Each crate gets its behaviour from its own issue, and nothing is built ahead
of one (`.claude/memory/owner-work-style.md`). The `openehr-*` family is pinned
in `docs/VERSIONS.md`, at a release that carries the federation gaps
FerroFED raised, the open ITS-REST `Error` a node's consent refusal `code`
is read from, the AQL function classification the rewrite reads, the
`Authorization` value configuration load checks credentials with, and the RM
model's primitives, `Ordered` marker and reference targets the rewrite orders
keys by.

The specification is pinned at its 1.0.1 release, the erratum of
2026-11-01 to the 1.0.0 release the openEHR SEC approved on 2026-10-12, at
upstream's untagged commit `b4242c2` by the owner's exception of 2026-11-02
(#1478, retired by #1970 when upstream tags it;
`.claude/memory/spec-pin-1-0-0.md`). Every requirement (`N#`) and
conformance point (`CP-#`) cited here is cited against that text.

## The two layers

- **FerroFED generates nothing of its own** (owner decision 2026-10-01,
  `docs/architecture.md` §10). The specification's two JSON Schemas,
  `federated-result-set.schema.json` (the result envelope with
  `meta.federation`) and `options-root.schema.json` (the `OPTIONS {base}/`
  self-description), get hand-written types in `openehr-federation`, held to the
  vendored schemas by validation, drift and semantic tests
  (`.claude/rules/codegen.md`). A file marked `// @generated` anywhere is
  off-limits: change the generator and regenerate.
- **Taken from the published `openehr-*` crates, never re-implemented**
  (owner rulings 2026-10-01). FerroEHR publishes them, and the whole family
  is the model:
  - `openehr-its`: the ITS-REST client face as its generated axum server
    traits (`rest-server` feature), dispatch to each node through its
    generated client (`rest-client` feature), and the canonical JSON;
  - `openehr-query`: AQL, parsed and re-emitted by `printer::to_aql` for the
    subject and identifier rewrite;
  - `openehr-base` and `openehr-rm`: the typed identifiers (`HIER_OBJECT_ID`,
    `OBJECT_VERSION_ID`, the `ehr_id` and `system_id` forms) behind §12
    follow-up routing, and every RM fact.

  A gap in those crates is
  filed as an issue in FerroEHR's tracker and never worked around here
  (`.claude/memory/openehr-crates-are-the-model.md`).
- **Hand-written and the product.** The federation semantics on top of those
  crates: the AQL rewrite and the identifier-hygiene gate, the
  identity-resolution and localization clients, the registry, the fan-out
  and completeness policy, the result merge, follow-up routing, the
  stored-query registry, and the outbound credentials. Modern idiomatic Rust
  of our own design, with the Federation Tier specification, openEHR AQL and
  ITS-REST, and the bound IHE profiles as the authority.

Spec crates are split from the app crates, as in every Ferro product. Nothing
is published to crates.io for now, and publishing is a one-line switch:
`publish = false` inherited from `[workspace.package]`, with the lane, the dry
run and the version guard built from v0.0.2 (`.claude/memory/crate-split.md`,
`.claude/rules/crates-publishing.md`). A capability whose home is another
product belongs there (`.claude/memory/family-architecture.md`): patient
identity and the IHE profiles are FerroPIX's, reached as a client over the
`ihe-iti` and `nl-generic-functions` crates it publishes, and the
interoperability component `eehrxf` is FerroBRIDGE's, published from there and
required by a caret range here (#1772); every component change is made there (A76, #813, owner
decision 2026-10-28: no component is built in one product to be moved later).

## Repo map

Today:

- `docs/specs/`: the vendored corpora, one directory per corpus with a
  `PROVENANCE.md`, fetched by `scripts/vendor/*.sh` from the pins in
  `docs/VERSIONS.md`: the Federation Tier specification source (CC0) and the
  reference implementation (Apache-2.0), the second as evidence, never an
  oracle and never a source of code.
- `docs/architecture.md` (the design of record), `docs/VERSIONS.md` (the pin
  matrix), `docs/ci-cd.md` and `docs/release.md` (the cut checklist).
- `conformance/`: the §17 matrix (`matrix.tsv`), the §16.3 tracks
  (`tracks.tsv`), the requirements and obligations tables, the golden AQL
  pass list and the rendered badges.
- `changelog.d/`: one changelog fragment per change, assembled into
  `CHANGELOG.md` at the cut by `scripts/release/changelog.sh --assemble`.
- `scripts/checks/`: the committed guards CI tier 1 runs (comment style, file
  length, duplication, versions, copyright holder, conformance matrix,
  obligations, e2e placement and the rest), each runnable by hand. Three are
  ratchets that only tighten (#1463): `file-length.sh` holds a hand-written
  Rust file or book page to 1000 lines and a listed one to its recorded
  length, `duplication.sh` holds the duplicated lines jscpd measures to
  `duplication.count`, and `forwarders.sh` holds the functions that only
  forward to another call, which `tools/forwarders` finds, to
  `forwarders.count` (#1484); a fall passes with a note and
  `ratchet-tighten.yml` lowers the counts on `main` (#1467).
  `module-cycles.sh` fails when two top-level modules of `ferrofed-server`
  import each other outside tests, which `tools/module-cycles` finds
  (#1580). `fast.sh` is the default local gate: rustfmt, clippy, nextest
  and the doctests over the packages a change reaches, and the fuzz crate
  when it builds on one (#1858).
- `scripts/conformance/`: the matrix, obligations and per-run report
  (`report.sh`) scripts.
- `scripts/release/`: the changelog assembly, the crates.io lane, the
  release staging and the seed data.
- `scripts/gh/`: the tracker helpers (`rel.sh`, `project.sh`, `fields.sh`,
  `labels.sh`, `migrate-fields.sh`).
- `scripts/vendor/`: the fetch scripts for every vendored corpus.
- `website/`: the mdBook (`book/`) and the landing page (`landing/`);
  `deploy/`: the release compose file and the example Kubernetes manifests;
  `docker/`: the gateway and console Dockerfiles, the quickstart
  configuration of the root `compose.yaml` and the node database init script.
- `assets/brand/`: the mark and the "Azure & Iron" tokens.
- `.github/`: issue and pull-request templates, CODEOWNERS, Dependabot, and
  the workflows. `ci.yml` runs the tier-1 guards and the Rust tier, and its
  `conclusion` job is the single required check on `main`.
- `.claude/`: the working discipline. `rules/`, `hooks/`, `skills/`,
  `agents/`, `memory/`.
- Root markdown: this file, `README.md`, and the community and governance set.

The Cargo workspace (#28), the crate map of `docs/architecture.md` §11:

- `crates/`: the libraries a third party could use, each named for the
  specification it implements, never `ferrofed-*`
  (`.claude/memory/published-crate-naming.md`), with one feature per layer or
  profile, publishable only through the workspace `publish` switch (#106):
  `openehr-federation` (the Federation Tier: the wire additions always on, the
  rewrite on `openehr-query` behind `aql`, the cross-node merge behind
  `merge`). The interoperability component of Regulation (EU) 2025/327
  (decision A58) is `eehrxf`, published from FerroHEALTH/FerroBRIDGE since
  #813 and required by a caret range, as every FerroBRIDGE crate is since it
  follows Cargo SemVer (`eehrxf = "0.3.2"`, #1772); FerroFED keeps no copy, and
  its own half is `app/ferrofed-eehrxf` below. The logging component,
  `ehds-logging` (the Annex II 3.2 access record, the
  Art 14(1) categories and their map, the sink, and `balp`; #623), and the
  ATNA and BALP audit, `ihe-atna`, come from the family's audit library,
  FerroHEALTH/FerroAUDIT, pinned exactly from crates.io (#1176). The two
  components never list each other and only `ferrofed-server` links both,
  which the architecture test holds. A library may depend on another library,
  and `cargo package` resolves it
  through a `patch.crates-io` per depended-on member while the switch is off
  (`.claude/rules/crates-publishing.md`). The library crates depend on
  nothing in FerroFED, so FerroPIX can use them as they are.
- `ihe-iti` (the IHE ITI profiles: `pixm`, `pdqm`, `mcsd`, `pmir`, `xcpd`,
  with `balp` and `atna` turning on their audit records, written through
  `ihe-atna`, every client built from `ferrohealth-smart`'s outbound client)
  is owned and published by FerroHEALTH/FerroPIX since #367; FerroFED
  requires it by a caret range from crates.io (`ihe-iti = "0.2.0"`) and
  keeps no copy.
- `nl-generic-functions` (the Annex B functions: `nvi`, `mitz`, `lrza`,
  `nuts-auth`, and the BSN naming systems in `identification`) is owned and published by
  FerroHEALTH/FerroPIX since #367; FerroFED requires it by a caret range
  from crates.io (`nl-generic-functions = "0.2.0"`) and keeps no copy.
- `app/`: FerroFED's own glue, each a hard `publish = false`:
  `ferrofed-registry` (members, learned maps, incidents, the definition store
  trait), `ferrofed-operator-api` (the operator and console wire documents
  the gateway writes and the console reads, #1489), `ferrofed-identity` (laid out by role and binding: `role/` holds
  the seams and `PatientRef`; `ihe/`, behind feature `ihe`, the PIXm
  resolver and localizer, the PDQm demographics step, the XCPD localizer,
  the PMIR identity feed and the mCSD directory over `ihe-iti`, and the
  audit recorders over `ihe-atna`, whose BALP recorder alone builds under
  feature `balp`, which
  `ihe` implies and the server always enables (#697); `nl/`, behind feature `nl`, the NVI localizer and the Mitz
  pre-filter over `nl-generic-functions`; and at the top `session`, the
  resolution bindings per verified caller, and `profile`, the deployment
  profile; every adapter takes the `ferrohealth-smart` outbound client its
  endpoint table builds),
  `ferrofed-eehrxf` (the interoperability component's federation half:
  the patient summary's stored section queries, read-only under the
  reserved namespace `eu.ferrofed.eehrxf`, which the server's registry
  holds beside a deployment's own, #776) and
  `ferrofed-engine` (dispatch, fan-out, budgets, follow-up routing, the calls
  to one node in `single_node`, the gateway's own token in
  `onward::upstream` over `ferrohealth-smart`'s client-credentials client,
  the onward grants in `onward::grant` (the onward lane over
  `ferrohealth-smart`'s onward client, #1703, and the Nuts grant under
  `nl`), the node's DPoP prover in `dispatch::dpop`, with no OAuth 2.0
  client of its own (#1749), the signed caller token in `conveyance`, and the outbound identifier-hygiene gate every
  request to a node passes, #45).
- `app/ferrofed-server`: the `ferrofed` binary, a thin `main.rs` over the
  library run path; never published. It carries the server shape (#29):
  `serve`, `config check`, `admission check`, `healthcheck` and `conformance run`
  (`conformance`, #546: the Connectathon tracks scored against a configured
  deployment, with `--node-profile` for the members), the regional bindings
  behind one `Binding` trait, each a module under `binding/` and a default
  feature (`binding-ihe`, `binding-nl`; #489), the TOML and `FERROFED__` environment
  configuration with `_file` secrets and per-endpoint outbound credentials,
  the console, the panic line, the request log that carries no body, query
  text, header value or unmatched path, and the one OpenTelemetry provider of
  `ferrohealth-telemetry` (#1397), the shared health family of `ferrohealth-server` over
  `ferrohealth-health`'s indicator registry (#1397), the
  `tower-http` stack, the listeners with their TLS and the bounded drain of
  `ferrohealth-server` (#1397), and the startup banner. The gate
  (`auth`, #80) authenticates every caller before the façade reads the
  request, and the gateway's JWK Set is served at
  `{base}/.well-known/jwks.json` (#81). The façade (#38) serves the federated AQL query, the stored queries the gateway holds
  (`stored`: an embedded, a PostgreSQL and a read-only files backend), with
  FerroFED's own PostgreSQL store under `[store]` (`store`: its migrations,
  readiness indicator and sweep, #1636), which a contact-point issuer and the
  `postgres` backend need, the
  follow-up routes to the owning node, the definition routes to a chosen
  node, and `OPTIONS {base}/`, with its two `serde_json::Value` seams
  (`facade::intake`, `facade::cells`); an ITS-REST path it does not serve
  answers `501`. The admin listener (`[metrics] listen`) carries the
  OpenTelemetry metrics, exported as Prometheus and, under
  `[telemetry.otlp]`, over OTLP with the spans, behind an
  optional scrape token or mutual TLS, and the stored-query drift repair,
  admitted through the client authentication gate for the issuer's
  operator scope and recorded with the operator who ran it (#635, #767),
  and the running configuration with its save of settings (`running`,
  #1235).
- `app/ferrofed-console`: the operator console, the `ferrofed-console` binary
  and its own image (#275, decisions A55, A104 and A111); never published.
  Every page is one document its server draws whole with FerroUI
  (`ferrohealth-ui`, pinned exactly), with no Leptos and no WebAssembly
  (#1457): `routes` draws the page an address names, in the `shell` for a
  signed-in operator, each page reading the gateway through `read` before it
  draws, and each action a plain form that posts to the page it sits on,
  which the server draws again with the answer (`act`); FerroUI's script
  modules, served by the library's own asset router, add the in-page
  behaviour, and every page reads and every action posts with them
  refused. `visit` holds what the server read of the request for the
  request's task. A pure HTTP client of the gateway (`gateway`, on
  `openehr-its` `rest-client`) that links no part of it; the operator's
  OpenID Connect session is held on its server (`session`, `oidc`), the code
  exchanged and the ID Token checked before a session begins. The pages
  (`pages`) show the dashboard, the topology, the organisations and their
  nodes, the services, the integrity incidents, the stored queries and the
  self-description, through the gateway's read-only operator surface
  (`{base}/operator/`, behind the `[auth] product_scope`, #1174), the query
  console runs AQL and the stored queries with every node's status and
  completeness shown (#277), and the configuration page changes the
  gateway's configuration revisions (`configure`). Its discipline is
  `.claude/rules/console-ui.md`. FerroUI leads its UI: its components,
  design and component rules come from the FerroUI repository at the tag of
  the pin, and a missing part is a FerroUI issue, never a local copy.
- `tools/ferrofed-testkit`: test support; never published. The pin-matrix
  reader, the wiremock `Server` that drops off the runtime (`mock`, #361),
  and the harness of `docs/architecture.md` §13 (#39): FerroEHR nodes pinned
  by digest behind the `FERROFED_E2E` gate (`containers`, #155), with EHRbase
  as the node profile's second CDR product (`containers::ehrbase`, #549) and
  Keycloak as the issuer the production guide's recipe is applied to, read
  from the book page as it prints it (`containers::keycloak`, #724); the
  capturing and fault proxy in front of each node (`proxy`, with
  `Fault::Reply` for an ITS-REST `Error` answer and
  `Bind::EveryInterface` for a proxy a container dials); the
  synthetic seed builder that writes over ITS-REST alone inside the
  `urn:oid:2.999` example arc (`seed`); where the node profile findings of
  each product are written (`node_profile`, #93); and the reference
  implementation built from its vendored source, digest-pinned, as the second
  gateway of the differential run (`reference`, #94)
  (`.claude/memory/e2e-gate.md`). The Connectathon scenarios themselves sit in
  `app/ferrofed-server/tests/it/e2e/` (#545). It also carries the harness
  services every binding is tested against: FerroPIX, the PIX Manager of the
  end-to-end lane (`containers::ferropix`), with fixed ITI-83, ITI-78 and
  ITI-119 answers for the offline suites (`pixm_stub`, `pdqm_stub`, #1360),
  the XCPD responding gateway, mCSD directory, PMIR Registry, Audit Record
  Repositories, NVI, Mitz, Nuts node, the OAuth 2.0 and FAPI 2.0
  authorization servers and a national contact point's national connector
  (`national_connector`, #735), with a mutual-TLS front. Its test binary also holds
  the operator console's browser journeys behind the `FERROFED_JOURNEYS` gate
  (#608): headless Chrome over WebDriver against a running gateway and console.
- `tools/forwarders`: the forwarder ratchet's detector; never published.
  It parses the hand-written Rust with `syn` and lists every function whose
  whole body forwards to one other call, for `scripts/checks/forwarders.sh`.
- `tools/module-cycles`: the module-cycle guard's detector; never published.
  It parses a crate with `syn` and lists the pairs of top-level modules that
  import each other outside tests, for `scripts/checks/module-cycles.sh`.
- The root `Cargo.toml` carries the lint set, the release profile, the
  `openehr-*` family as one pin group and the `publish` switch; `deny.toml`,
  `clippy.toml`, `rustfmt.toml` and `rust-toolchain.toml` sit beside it.
  Integration tests follow the `tests/it/` single-binary convention
  (`.claude/rules/testing.md`).

## Issue workflow (the loop)

The tracker is GitHub Issues; the open issue list is the worklist
(`.claude/rules/issue-workflow.md`). The type (Bug, Feature, Task), the
priority (Urgent, High, Medium, Low) and the effort (High, Medium, Low) are
GitHub's native issue type and the organisation's `Priority` and `Effort`
issue fields, never labels, set with `scripts/gh/fields.sh`; an issue is
filed with `scripts/gh/fields.sh new` so it carries all three from the start.
A Task carries exactly one work-kind label (documentation/chore/refactor/
perf/test/ci), and domain labels are added as needed (`spec:federation`,
`spec:openEHR`, `spec:IHE`, `spec:NL-GF`). Read an issue with
`gh issue view <n> --json title,body,comments`, never `--comments`, which
prints nothing for an issue without comments. Milestones are releases, starting
at v0.0.1 and stepping by a patch number
(`.claude/memory/milestones-0-0-x.md`). Record progress on the issue (tick
criteria, comment); a PR declares `Closes #N`. New work found while working an
issue is filed and fixed before the next unit starts. Native sub-issue and
dependency edges are set only with `scripts/gh/rel.sh`; the "FerroFED Roadmap"
board is a view over the tracker, written only with `scripts/gh/project.sh`
(`.claude/rules/issue-relationships.md`, `.claude/rules/project-board.md`).
The SessionStart hook prints the open issue list with `<Type/Priority>` after
each title, and `/next-task` takes the highest priority first, the oldest
first within a priority.

## Model orchestration (workflows and subagents)

**When the session runs on Fable (currently Fable 5.1, effort `high`), Fable
is the orchestrator, not the implementer.** Fable plans, coordinates, reviews,
and does the taste- and intelligence-heavy work itself; it fans
implementation out to subagents through the `Agent` tool (`model: 'opus'`,
which resolves to the newest Opus, currently Opus 5.5; the `.claude/agents/*`
definitions with `model: opus` pick it up), never a `Workflow`, whose agents
cannot be messaged (owner 2026-11-01). Every subagent runs on Opus, research
and review included; never pass `fable` to `Agent` (owner 2026-10-30). The main loop does not auto-delegate, so this section is the
standing instruction that it should.

The win is context isolation and parallelism. It is an intelligence downgrade
per worker, so delegate by the nature of the work, never reflexively.

| model | cost | intelligence | taste |
|-------|------|--------------|-------|
| fable | 2    | 9            | 9     |
| opus  | 4    | 8            | 8     |

Only these two models are used. Sonnet and Haiku are not used in this
project; never pass them to `Agent` or `Workflow`, even for a mechanical pass.

- **Orchestrator (Fable, high):** owns the issue loop, architecture and design
  decisions, spec-conformance judgement, and the hard bespoke logic (the AQL
  rewrite and hygiene gate, the completeness and merge semantics, follow-up
  routing over the four identifiers).
- **Delegate to Opus subagents:** bulk or parallelizable implementation on a
  clear spec, file-heavy investigation, and codebase analysis. Up to ten
  implementation workers at once across the family (owner cap since
  2026-11-03, replacing the earlier caps of two, four and five). Tell workers not to spawn
  their own subagents, and to write any long report to a scratchpad file
  (`.claude/memory/subagent-reports-to-file.md`).
- **Reviews:** an independent read before committing a subsystem, especially
  spec and wire conformance. Spec questions go to `spec-researcher`; bounded
  implementation to `implementer`. Both are handed the governing spec
  sections in the prompt. Operator console work goes to `ui-implementer`, and
  `ui-reviewer` reads a console subsystem before it is committed.

Discipline is unchanged for subagents: they obey the hard rules below.

## IMPORTANT hard rules

- **Family rules carried in `.claude/memory/` (index `MEMORY.md`).** A removal
  aid (a named refusal of a removed or renamed config key, flag, env variable,
  route or format) lives exactly one release and the next release deletes it
  fully; no legacy support is ever kept (owner hard rule 2026-11-09,
  `no-compat-shims.md`). Build the proper solution even when it breaks an API
  and moves the minor version (owner 2026-11-08, `no-half-solutions.md`). A
  production value map goes only through the terminology server, never an
  embedded copy (owner 2026-11-09, `terminology-via-server.md`). Where the
  EHDS Regulation conflicts with an openEHR or other standard's text, the
  Regulation wins, and EHDS work is never reverted to pass a conformance row
  (owner 2026-11-09, `ehds-above-openehr.md`).
- **The specification text is the oracle.** The conformance authority is the
  Federation Tier specification at its pinned version, the openEHR AQL and
  ITS-REST specifications, and the IHE profiles the specification binds,
  never memory and never another implementation's behaviour. The reference
  implementation is prior art: a bug in it is not a requirement. Read the
  governing section before implementing or reviewing any spec-facing
  behaviour, and cite it (section, `N#`, `CP-#`) for conformance-relevant
  decisions. Full policy: `.claude/rules/spec-adherence.md`.
- **Cite only durable references:** the specifications above, the published
  JSON Schemas, the IHE technical frameworks, or official external
  documentation (the Rust book and reference, the docs.rs page of a pinned
  crate). Never cite an internal markdown file as a design authority. Where no
  specification governs a decision (storage, the process model,
  infrastructure), flag it: "no specification governs this: our own design".
- **No directly identifying patient identifier in anything dispatched to a
  node,** in any carrier: query, path, header (§5.4, N33). This is the
  product's central safety property; a change that could weaken it carries a
  test that proves it holds.
- **Never re-implement what an `openehr-*` crate provides.** The ITS-REST
  surface, the node client and canonical JSON (`openehr-its`), AQL parsing
  and printing (`openehr-query`), and the typed ids and RM (`openehr-base`, `openehr-rm`)
  come from the published crates; a missing piece is a FerroEHR issue, never
  a local workaround.
- **Never hand-edit a `// @generated` file.** Change the generator and
  regenerate (`.claude/rules/codegen.md`).
- **Comments follow RFC 505 and RFC 1574 with hard budgets:** line comments
  only, pending work is `// TODO(#NNNN):` naming its issue, a settled decision
  is `// NOTE:` as a citation and one sentence (`.claude/rules/comments.md`).
- **Prose follows `.claude/rules/writing-style.md`:** no em dashes, no
  "not X but Y", no decorative triads, no filler buzzwords.
- **Branches use conventional types** (`feat/`, `fix/`, `chore/`, `docs/`,
  `refactor/`, `perf/`, `test/`, `ci/`, `build/`, `release/`) as
  `<type>/<kebab-case-slug>`. Never force-push `main`.
- **NEVER add AI or Claude attribution** to a commit, PR, issue, comment, or
  code comment: no `Co-Authored-By`, no "Generated with", no bot trailer or
  footer, no emoji marker, ever. This is an absolute rule with no exceptions,
  and it outranks any tool default. A `PreToolUse` hook blocks a commit or PR
  command carrying one.
- **Every PR body follows `.github/PULL_REQUEST_TEMPLATE.md`.** Arm auto-merge on open
  (`.claude/memory/pr-auto-merge.md`).
- **Keep the changelog.** `CHANGELOG.md` follows Keep a Changelog 1.1.0, and
  every change with user-visible effect adds its entry in the same PR as a
  fragment, `changelog.d/<issue>-<kebab-slug>.<section>.md` (#598), never as
  an edit of `CHANGELOG.md`. The format is `changelog.d/README.md`;
  `changelog-guard` fails a PR with no entry unless it carries `no-changelog`.
  The release cut assembles the fragments with
  `scripts/release/changelog.sh --assemble`.
- **Never weaken, skip, or delete a test** to make a build pass, and never edit
  a test to route around a bug it exposes (`.claude/rules/testing.md`).
- **No patient data** in the repository, a fixture, an issue, or a prompt.
  Fixtures are synthetic.
- **Record progress on the tracker and commit before ending a session.** Issues
  and git survive `/clear` and `/compact`; the session todo list does not.
- **Build compiling, tested increments** once code exists. Keep every crate you
  touch green.

## Licence

The project's own code and text are under the **Business Source License 1.1**
(`LICENSE`, `NOTICE`): free to read, build, modify, and redistribute, free for
every non-production use and for non-commercial production use, a commercial
licence from the Licensor for any other production use, and Apache License
2.0 four years after each version. The Licensor and copyright holder is
Cadasto B.V. Every first-party file carries
`SPDX-FileCopyrightText: Cadasto B.V.` and
`SPDX-License-Identifier: BUSL-1.1` in its header. A generated file keeps its
`// @generated … DO NOT EDIT.` banner on the first line and carries the two
SPDX lines under it, written by its emitter. A contribution is licensed under
the same licence and grants the Licensor the relicensing right in
CONTRIBUTING.md § Licensing of contributions; opening the pull request is
the contributor's acceptance of those terms. Vendored
specifications and third-party material keep their upstream terms, recorded in
a `PROVENANCE.md` beside each vendored tree
(`.claude/rules/vendored-inputs.md`). The decision is recorded in
`.claude/memory/license-busl.md`.

## Working discipline (`.claude/`)

Path-scoped rules load on demand when files in their scope are read; the rest
apply always. Read the relevant one before working in that area.

- `.claude/rules/writing-style.md`: no AI tells in any prose.
- `.claude/rules/rust-style.md`, `reliability.md`, `comments.md`, `testing.md`:
  the Rust engineering discipline.
- `.claude/rules/spec-adherence.md`: the Federation Tier specification,
  openEHR AQL and ITS-REST, and the bound IHE profiles as the oracles.
- `.claude/rules/codegen.md`: no generator of our own; the hand-written wire
  types held to the schemas, and the rules if a generator is ever justified.
- `.claude/rules/vendored-inputs.md`: every external corpus is fetched by a
  committed `scripts/vendor/*.sh`, vendored verbatim, provenance-stamped.
- `.claude/rules/ci-cd.md`, `ai-code-review.md`, `crates-publishing.md`: the
  workflow-security discipline, the advisory-analyzer policy (SonarQube Cloud,
  CodeQL), and the crates.io rules behind the `publish` switch.
- `.claude/rules/console-ui.md`: the operator console's discipline.
  FerroUI leads the UI: the rule, the `ui-implementer` and `ui-reviewer`
  agents and the `/ferroui-lookup` and `/ui-gates` skills read FerroUI's
  consumer guide and component rules first and keep only what is particular
  to this console.
- `.claude/rules/issue-workflow.md`, `issue-relationships.md`,
  `project-board.md`: the tracker work style.
- Skills: `/spec-lookup`, `/ferroui-lookup`, `/ui-gates`, `/next-task`,
  `/phase-done`, `/phase-status`.
- Agents: `spec-researcher`, `implementer`, and for the console
  `ui-implementer` and `ui-reviewer` (all on Opus).
- Memory: `.claude/memory/`, indexed by `MEMORY.md`, tracked and shared
  (`.claude/memory/memory-lives-in-repo.md`).

## Sibling projects

FerroFED is one of five FerroHEALTH products, each its own server reached
only over a published protocol (`.claude/memory/family-architecture.md`).
FerroPIX is the master patient index with the IHE profiles, which the
gateway calls as a client for patient identity. FerroEHR (`../FerroEHR`) is
the reference openEHR CDR and publishes the
`openehr-*` crates this gateway builds on (`openehr-its`, `openehr-query`,
`openehr-base`, `openehr-rm`); FerroTERM (`../FerroTERM`) and
FerroBRIDGE (`../FerroBRIDGE`) carry the working discipline this repository
was set up from. All are read-only prior art from here
(`.claude/memory/sibling-projects.md`). Public documents may name the family
(`.claude/memory/family-naming-allowed.md`); the gateway still works against
any CDR over ITS-REST, and no sibling is a compile-time dependency beyond the
published crates.

## References

- The Federation Tier specification (rendered):
  <https://syntaric.github.io/openehr-federation-spec/>, source
  <https://github.com/syntaric/openehr-federation-spec>
- The reference implementation: <https://github.com/syntaric/openehr-federation-ref>
- The openEHR ITS-REST specification:
  <https://specifications.openehr.org/releases/ITS-REST/latest/>
- The openEHR Archetype Query Language:
  <https://specifications.openehr.org/releases/QUERY/latest/AQL.html>
- IHE ITI profiles (PIXm, PDQm, XCPD, PMIR, mCSD):
  <https://profiles.ihe.net/ITI/>
- The Netherlands Generic Functions IG (Annex B):
  <https://build.fhir.org/ig/nuts-foundation/nl-generic-functions-ig/>
- The tracker: `gh issue list --state open`. Issue #16 (closed) carries the
  research program that produced `docs/architecture.md`.
