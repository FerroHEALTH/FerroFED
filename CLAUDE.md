<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CLAUDE.md

**FerroFED** is a pure-Rust openEHR federation gateway: a transparent
ITS-REST intermediary in front of many openEHR CDRs. A client sends it an
ordinary AQL query and never learns it was federated. The gateway resolves the
patient outside AQL, through an identifier cross-reference service, to a set of
`{node, local ehr_id}`; it sends standard, non-federated AQL to each node,
scoped to that node's own `ehr_id`; it merges what comes back with each node's
provenance; and it routes follow-up reads and writes to the owning CDR. No
directly identifying patient identifier travels in a dispatched query. It holds
no clinical data of its own. It implements the openEHR Federation Working
Group's *Proposal for Federation Tier with AQL* (the Federation Tier
specification), and it reaches every node over the openEHR ITS-REST API.

The name follows the Ferro family (FerroEHR, FerroTERM, FerroBRIDGE, and the
rest of FerroHEALTH). FerroFED in prose, `ferrofed` in identifiers.

## Status: the design phase

There is no code and no Cargo workspace. The design is the output of the
research program on issue #16, which produces `docs/architecture.md`
in v0.0.1: how the AQL rewrite and the identifier-hygiene gate sit on the
published `openehr-query` AQL model, which identity binding comes first (IHE
PIXm for the cross-reference, mCSD for addressing, the Dutch Generic Functions
of Annex B as the regional alternative), where the registry lives, how the
result merge, `DISTINCT`, `ORDER BY` and `LIMIT` are made correct across nodes,
and what the acceptance instrument is. Read that issue before proposing
anything structural. Nothing is scaffolded before its issues are filed and the
research that fixes it has closed (`.claude/memory/owner-work-style.md`).

The specification is a release candidate. The vendored pin is v0.9.0 at
commit `7162d0c`, and the 1.0 release is expected the week of 2026-10-08; the
re-pin is its own issue (`.claude/memory/spec-pin-0-9-0-rc.md`). Every
requirement (`N#`) and conformance point (`CP-#`) cited here is cited against
the pinned text until that issue lands.

## The two layers

- **Generated where a machine-readable source exists.** The specification
  publishes two JSON Schemas, `federated-result-set.schema.json` (the result
  envelope with `meta.federation`) and `options-root.schema.json` (the
  `OPTIONS {base}/` self-description). Whether these are generated into Rust
  types or validated against hand-written ones is research on issue
  #16 (`.claude/rules/codegen.md`). Either way a file marked
  `// @generated` is off-limits: change the generator and regenerate.
- **Taken from the published `openehr-*` crates, never re-implemented**
  (owner rulings 2026-10-01). FerroEHR publishes them, and the whole family
  is the model:
  - `openehr-its`: the ITS-REST client face as its generated axum server
    traits (`rest-server` feature), dispatch to each node through its
    generated client (`rest-client` feature), and the canonical JSON;
  - `openehr-query`: AQL, parsed and re-emitted by `printer::to_aql` for the
    subject and identifier rewrite;
  - `openehr-sdt`: the SMART on openEHR scope grammar for the §13 security
    handoff, and the simplified formats and their validation if ever needed;
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

Spec and generated crates are split from the app crates, as in every Ferro
product; whether the library crates are published to crates.io is open
(`.claude/memory/crate-split.md`).

## Repo map

Today:

- `docs/specs/`: the vendored corpora, one directory per corpus with a
  `PROVENANCE.md`, fetched by `scripts/vendor/*.sh` from the pins in
  `docs/VERSIONS.md`: the Federation Tier specification source (CC0) and the
  reference implementation (Apache-2.0), the second as evidence, never an
  oracle and never a source of code.
- `scripts/checks/`: the committed guards, starting with
  `contribution-licence.sh` (the pull-request licence checkbox).
- `scripts/gh/`: the tracker helpers (`rel.sh`, `project.sh`, `labels.sh`).
- `scripts/vendor/`: the fetch scripts for every vendored corpus.
- `assets/brand/`: the mark and the "Azure & Iron" tokens.
- `.github/`: issue and pull-request templates, CODEOWNERS, Dependabot, and
  the workflows. `ci.yml` runs the tier-1 guards with the Rust tier gated
  until a workspace exists, and its `conclusion` job is the single required
  check on `main`. `contribution-licence.yml` runs the licence guard.
- `.claude/`: the working discipline. `rules/`, `hooks/`, `skills/`,
  `agents/`, `memory/`.
- Root markdown: this file, `README.md`, and the community and governance set.

Planned, decided by the research on issue #16: an
`app/ferrofed-server` binary (the gateway, mounting the `openehr-its`
`rest-server` traits as its client face), library crates under `crates/` for
the hand-written federation engine over `openehr-query` and the
`openehr-its` `rest-client`, a spec-derived crate for the two published
schemas, and a testkit tool crate. When the workspace lands, each crate
carries its own `CLAUDE.md` and this section becomes the crate map.

## Issue workflow (the loop)

The tracker is GitHub Issues; the open issue list is the worklist
(`.claude/rules/issue-workflow.md`). One type label per issue
(bug/enhancement/documentation/chore/refactor/perf/test/ci), one priority
label (P0 to P3), and domain labels as needed (`spec:federation`,
`spec:openEHR`, `spec:IHE`, `spec:NL-GF`). Milestones are releases, starting
at v0.0.1 and stepping by a patch number
(`.claude/memory/milestones-0-0-x.md`). Record progress on the issue (tick
criteria, comment); a PR declares `Closes #N`. New work found while working an
issue is filed and fixed before the next unit starts. Native sub-issue and
dependency edges are set only with `scripts/gh/rel.sh`; the "FerroFED Roadmap"
board is a view over the tracker, written only with `scripts/gh/project.sh`
(`.claude/rules/issue-relationships.md`, `.claude/rules/project-board.md`).
The SessionStart hook prints the open issue list.

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
  clear spec, file-heavy investigation, and codebase analysis. At most two
  implementation workers at once (an owner cap). Tell workers not to spawn
  their own subagents, and to write any long report to a scratchpad file
  (`.claude/memory/subagent-reports-to-file.md`).
- **Reviews:** an independent read before committing a subsystem, especially
  spec and wire conformance. Spec questions go to `spec-researcher`; bounded
  implementation to `implementer`. Both are handed the governing spec
  sections in the prompt.

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
- **Keep the changelog.** `CHANGELOG.md` follows Keep a Changelog 1.1.0: every
  change with user-visible effect adds an entry under `[Unreleased]` in the
  same PR. Releases are cut from the changelog.
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
Vernum Projecten B.V. Every first-party file carries
`SPDX-FileCopyrightText: Vernum Projecten B.V.` and
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
- `.claude/rules/codegen.md`: the generated-versus-hand-written rule, pending
  the research that fixes the boundary.
- `.claude/rules/vendored-inputs.md`: every external corpus is fetched by a
  committed `scripts/vendor/*.sh`, vendored verbatim, provenance-stamped.
- `.claude/rules/ci-cd.md`, `ai-code-review.md`, `crates-publishing.md`: the
  workflow-security discipline, the advisory-analyzer policy (SonarQube Cloud,
  CodeQL), and the crates.io rules for when a publish is decided.
- `.claude/rules/issue-workflow.md`, `issue-relationships.md`,
  `project-board.md`: the tracker work style.
- Skills: `/spec-lookup`, `/next-task`, `/phase-done`, `/phase-status`.
- Agents: `spec-researcher`, `implementer` (both on Opus).
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
- The tracker: `gh issue list --state open`. Issue #16 carries the
  research program that produces `docs/architecture.md`.
