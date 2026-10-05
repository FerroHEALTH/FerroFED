<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md

**FerroFED** is a pure-Rust openEHR federation gateway: a transparent
ITS-REST intermediary in front of many openEHR CDRs. A client sends it an
ordinary AQL query and never learns it was federated. The gateway resolves the
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

## Status: building v0.0.9

v0.0.8 is released (2026-10-04). v0.0.7 (2026-10-03) carries definitions and
membership (§12.6, §12.7, §12b): definition requests routed to one chosen node, the
template fan-out, the stored-query registry with its storage backends and
drift repair, the admission check, the metrics surface, every credential held
in a type that never renders it, and a gateway-only compose file with example
configuration attached to every release. v0.0.6 carried the v0.0.4 to v0.0.6
milestones: the merged answer across nodes (§9 to §11), the ITS-REST surface
with follow-up routing (§7a, §12, §12a), and targeting with the
self-description (§8, §7a.2). v0.0.8 carries security and the bindings (§13
to §15, Annex A, Annex B): every caller is authenticated at the gateway
(#80); each node is reached with its own
credential, an OAuth 2.0 token for a client assertion the gateway signs, with
token exchange and DPoP (#81, #439), the Nuts grant (#88) or the FAPI 2.0
grant (#497), and is told the caller in a token the gateway signs (#82);
consent stays with the node, with the optional Step-1 pre-filter (#83) and
Mitz (#475); undirected patient queries are localized by XCPD (#85), PIXm or
the NVI (#87), with PDQm ahead of resolution (#487) and PMIR identity changes
dropping stale bindings (#147); the registry can be read from an mCSD
directory (#86) with LRZa addressing; every IHE transaction is audited over
ATNA ITI-20, the BALP records with a bounded spool (#418, #486, #512);
traces export over OpenTelemetry (#353); and the identity clients share one
TLS type with mutual TLS (#507).

v0.0.9, being built, carries the conformance program (#89): the Connectathon
tracks 1 to 9 as scenarios with a per-track and per-point report (#92), the
leakage and integrity suites of tracks 10 and 11 (#90, #91), the node
profile run against FerroEHR 4.3.3 and EHRbase 2.36.0 pinned by digest (#93,
#549), a differential run against the reference implementation (#94), and
`ferrofed conformance run`, which scores a configured deployment, runs the
node profile with `--node-profile` and reads the seed data each release
attaches (#546, #573); the conformance statement is #95. It also carries the
regional binding refactor: one `Binding` trait with the `binding-ihe` and
`binding-nl` features of the server, `ihe` and `nl` of the identity crate and
`nl` of the engine, and RFC 8414 in its own crate, `oauth-server-metadata`
(#489, #551); the identity crate laid out as `role/`, `ihe/` and `nl/`, and
the engine's onward grants under `onward::grant` beside `single_node`
(#587, #588); mutual TLS to a node with RFC 8705 client authentication and
certificate-bound tokens (#492, #560); DPoP proofs read from `openehr-its`
at 0.0.84 (#574); the FAPI 2.0 client key rotated with an overlap (#514); an
ES256 `[signing]` key beside ES384 (#513); the Nuts grant toward the NVI
(#539) and the gateway's did:web DID document (#503); the consent
non-disclosure setting `[federation.consent] disclose` (#493) and a
pre-filter that says it did not ask, with a closed `NotAsked` reason (#496,
#568); PIXm asked by POST (#494); the verified caller as the user agent of
each IHE audit record (#500); the operator console, `app/ferrofed-viewer`,
with its own image (#275) and the operator views (#276), with the operator
surface, sign-out, the bundle serving and the query console in flight (#583,
#584, #600, #277); the country research (#488); FerroFED's intended
purpose and EHDS classification (#520); changelog fragments (#598); and
Cadasto B.V. as the Licensor (#602). The re-pin to the specification's 1.0
release moved to v0.0.10 (#17, #354).
v0.0.10 is EHDS readiness (#519): FerroFED is an EHR system under Regulation
(EU) 2025/327, and its harmonised components are due before the dates the
Regulation applies. v0.0.11 plans configuration from the file and from the
console as one set of versioned revisions (#575). Each crate gets the rest of
its behaviour from its own issue, in milestone order.
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
FerroFED raised, the open ITS-REST `Error` the error vocabulary writes its
`code` into, the AQL function classification the rewrite reads, the
`Authorization` value configuration load checks credentials with, and the RM
model's primitives, `Ordered` marker and reference targets the rewrite orders
keys by.

The specification is a release candidate. The vendored pin is v0.9.0 at
commit `7162d0c`, and the 1.0 release is expected the week of 2026-10-08; the
re-pin is its own issue (`.claude/memory/spec-pin-0-9-0-rc.md`). Every
requirement (`N#`) and conformance point (`CP-#`) cited here is cited against
the pinned text until that issue lands.

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
  - `openehr-sdt`: the SMART on openEHR scope grammar for the §13 security
    handoff. It joined the workspace with onward OAuth 2.0 (#81), whose
    `oauth2` scope it checks, and client authentication (#80) reads each
    caller's scopes with it; its simplified formats and their validation are
    used if ever needed;
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
`.claude/rules/crates-publishing.md`). Identity bindings are built here first,
each as a crate that can move to FerroPIX later
(`.claude/memory/build-in-fed-first.md`).

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
  length, versions, copyright holder, conformance matrix, obligations, e2e
  placement, the contribution licence and the rest), each runnable by hand.
- `scripts/conformance/`: the matrix, obligations and per-run report
  (`report.sh`) scripts.
- `scripts/release/`: the changelog assembly, the crates.io lane, the
  release staging, the seed data and the console site bundle.
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
  `contribution-licence.yml` runs the licence guard.
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
  `merge`), `ihe-iti` (the IHE ITI profiles: `pixm`, `pdqm`, `mcsd`, `pmir`,
  `xcpd`, and the ATNA audit as `atna` and `balp`), `nl-generic-functions`
  (the Annex B functions: `nvi`, `mitz`, `lrza`, `nuts-auth`, and the BSN
  naming systems in `identification`) and `oauth-server-metadata` (RFC 8414,
  the issuer and the checks a client holds authorization server metadata to,
  with no feature; #551). A library may depend on another library
  (`nuts-auth` on `oauth-server-metadata`), and `cargo package` resolves it
  through a `patch.crates-io` per depended-on member while the switch is off
  (`.claude/rules/crates-publishing.md`). The library crates depend on
  nothing in FerroFED, so FerroPIX can use them as they are.
- `app/`: FerroFED's own glue, each a hard `publish = false`:
  `ferrofed-registry` (members, learned maps, incidents, the definition store
  trait), `ferrofed-identity` (laid out by role and binding: `role/` holds
  the seams and `PatientRef`; `ihe/`, behind feature `ihe`, the PIXm
  resolver and localizer, the PDQm demographics step, the XCPD localizer,
  the PMIR identity feed, the mCSD directory and the audit recorders over
  `ihe-iti`; `nl/`, behind feature `nl`, the NVI localizer and the Mitz
  pre-filter over `nl-generic-functions`; and at the top `session`, the
  resolution bindings per verified caller, `fhir`, the one IHE FHIR client
  and TLS type, and `dev`, the development cross-reference) and
  `ferrofed-engine` (dispatch, fan-out, budgets, follow-up routing, the calls
  to one node in `single_node`, the onward grants in `onward::grant`
  (client credentials, token exchange, FAPI 2.0, and the Nuts grant under
  `nl`) beside the DPoP and mutual TLS sender constraints, the signed caller
  token in `conveyance`, and the outbound identifier-hygiene gate every
  request to a node passes, #45).
- `app/ferrofed-server`: the `ferrofed` binary, a thin `main.rs` over the
  library run path; never published. It carries the server shape (#29):
  `serve`, `config check`, `admission check`, `healthcheck` and `conformance run`
  (`conformance`, #546: the Connectathon tracks scored against a configured
  deployment, with `--node-profile` for the members), the regional bindings
  behind one `Binding` trait, each a module under `binding/` and a default
  feature (`binding-ihe`, `binding-nl`; #489), the TOML and `FERROFED__` environment
  configuration with `_file` secrets and per-endpoint outbound credentials,
  the console, the request log that carries no body, query text, header value
  or unmatched path, the health family over an indicator registry, the
  `tower-http` stack, the bounded drain and the startup banner. The gate
  (`auth`, #80) authenticates every caller before the façade reads the
  request, and the gateway's JWK Set is served at
  `{base}/.well-known/jwks.json` (#81). The façade (#38) serves the federated AQL query, the stored queries the gateway holds
  (`stored`: an embedded, a PostgreSQL and a read-only files backend), the
  follow-up routes to the owning node, the definition routes to a chosen
  node, and `OPTIONS {base}/`, with its two `serde_json::Value` seams
  (`facade::intake`, `facade::cells`); an ITS-REST path it does not serve
  answers `501`. The admin listener (`[metrics] listen`) carries the
  OpenTelemetry metrics, exported as Prometheus and over OTLP, and the
  stored-query drift repair.
- `app/ferrofed-viewer`: the operator console, the `ferrofed-viewer` binary
  and its own image (#275, decision A55); never published. A Leptos app
  rendered on the server and hydrated in the browser, built by cargo-leptos,
  its two halves chosen by compilation target, never by Cargo feature. A pure
  HTTP client of the gateway (`gateway`, on `openehr-its` `rest-client`) that
  links no part of it; the operator's OpenID Connect session is held on its
  server (`session`, `oidc`), the code exchanged and the ID Token checked
  before a session begins. The operator views (`views`, #276) read the
  members, their health, the integrity incidents, the routing table, the
  stored queries and the self-description, the last four through the
  gateway's read-only operator surface (`{base}/operator/`, behind the
  issuer's `operator_scope`); the query console is #277. Its discipline is
  `.claude/rules/leptos-ui.md`.
- `tools/ferrofed-testkit`: test support; never published. The pin-matrix
  reader, the wiremock `Server` that drops off the runtime (`mock`, #361),
  and the harness of `docs/architecture.md` §13 (#39): FerroEHR nodes pinned
  by digest behind the `FERROFED_E2E` gate (`containers`, #155), with EHRbase
  as the node profile's second CDR product (`containers::ehrbase`, #549); the
  capturing and fault proxy in front of each node (`proxy`, with
  `Fault::Reply` for an ITS-REST `Error` answer and
  `CapturingProxy::start_reachable` for a proxy a container dials); the
  synthetic seed builder that writes over ITS-REST alone inside the
  `urn:oid:2.999` example arc (`seed`); where the node profile findings of
  each product are written (`node_profile`, #93); and the reference
  implementation built from its vendored source, digest-pinned, as the second
  gateway of the differential run (`reference`, #94)
  (`.claude/memory/e2e-gate.md`). The Connectathon scenarios themselves sit in
  `app/ferrofed-server/tests/it/e2e/` (#545). It also carries the harness
  services every binding is tested against: the PIX Manager, PDQm Supplier,
  XCPD responding gateway, mCSD directory, PMIR Registry, Audit Record
  Repositories, NVI, Mitz, Nuts node and the OAuth 2.0 and FAPI 2.0
  authorization servers, with a mutual-TLS front.
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
definitions with `model: opus` pick it up) or a `Workflow` (per-agent
`model`). The main loop does not auto-delegate, so this section is the
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
  clear spec, file-heavy investigation, and codebase analysis. At most four
  implementation workers at once (an owner cap, raised from two on 2026-10-02
  and to four on 2026-10-03). Tell workers not to spawn
  their own subagents, and to write any long report to a scratchpad file
  (`.claude/memory/subagent-reports-to-file.md`).
- **Reviews:** an independent read before committing a subsystem, especially
  spec and wire conformance. Spec questions go to `spec-researcher`; bounded
  implementation to `implementer`. Both are handed the governing spec
  sections in the prompt. Operator console work goes to `ui-implementer`, and
  `leptos-reviewer` reads a console subsystem before it is committed.

Discipline is unchanged for subagents: they obey the hard rules below.

## IMPORTANT hard rules

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
  and printing (`openehr-query`), the SMART on openEHR scopes
  (`openehr-sdt`), and the typed ids and RM (`openehr-base`, `openehr-rm`)
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
- **Every PR body follows `.github/PULL_REQUEST_TEMPLATE.md`** with the
  licensing box ticked, or `contribution-licence-guard` fails
  (`.claude/memory/pr-body-licence-checkbox.md`). Arm auto-merge on open
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
CONTRIBUTING.md § Licensing of contributions; the pull-request checkbox
records it and `contribution-licence-guard` enforces it. Vendored
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
- `.claude/rules/leptos-ui.md`: the operator console's Leptos discipline,
  carried from FerroEHR's and FerroTERM's viewers.
- `.claude/rules/issue-workflow.md`, `issue-relationships.md`,
  `project-board.md`: the tracker work style.
- Skills: `/spec-lookup`, `/leptos-lookup`, `/ui-gates`, `/next-task`,
  `/phase-done`, `/phase-status`.
- Agents: `spec-researcher`, `implementer`, and for the console
  `ui-implementer` and `leptos-reviewer` (all on Opus).
- Memory: `.claude/memory/`, indexed by `MEMORY.md`, tracked and shared
  (`.claude/memory/memory-lives-in-repo.md`).

## Sibling projects

FerroEHR (`../FerroEHR`) is the reference openEHR CDR and publishes the
`openehr-*` crates this gateway builds on (`openehr-its`, `openehr-query`,
`openehr-sdt`, `openehr-base`, `openehr-rm`); FerroTERM (`../FerroTERM`) and
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
