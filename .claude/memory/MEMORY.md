<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Memory index

## FerroFED decisions

- [Product scope](product-scope.md): the owner's product statement is the
  ceiling on what this repository may claim; a transparent ITS-REST gateway
  on the Federation Tier specification; everything structural was decided by
  the v0.0.1 research program into `docs/architecture.md`
- [Spec pin 0.9.0 RC](spec-pin-0-9-0-rc.md): specification at `7162d0c`,
  reference implementation at `92aff3c`, both vendored; 1.0 expected the week
  of 2026-10-08 with a dedicated re-pin issue; owner 2026-10-01
- [Crate split](crate-split.md): spec-derived crates split from the app crates
  as in every product; nothing published for now, publishing is a one-line
  `publish` switch with the whole lane built; owner 2026-10-01
- [Published crate naming](published-crate-naming.md): a publishable crate
  carries its specification's name, never ferrofed-*; one crate per
  specification with a feature per layer or profile; FED glue under app/;
  names claimed with 0.0.0 placeholders; owner 2026-10-01
- [Family naming allowed](family-naming-allowed.md): public documents may name
  FerroHEALTH and the siblings, unlike FerroBRIDGE; owner 2026-10-01
- [One setup PR](one-setup-pr.md): the opening setup lands as one large pull
  request, then one PR per issue resumes; owner 2026-10-01
- [Licence: BUSL 1.1](license-busl.md): Vernum Projecten B.V. is the Licensor
  (#1, #2) and the contribution terms, checkbox and guard landed (#3, #4)
- [Domain ferrofed.eu](domain-ferrofed-eu.md): a Pages setting on the family
  model, never a `CNAME` file
- [Sibling projects](sibling-projects.md): FerroEHR is the reference node and
  publishes the `openehr-*` crates; FerroBRIDGE and FerroTERM are the
  working-discipline template; all read-only from here
- [openEHR crates are the model](openehr-crates-are-the-model.md): the whole
  `openehr-*` family, nothing redone (`openehr-its` facade and node client,
  `openehr-query` AQL, `openehr-sdt` scopes, `openehr-base`/`openehr-rm` ids);
  a gap is a FerroEHR issue; owner 2026-10-01

## Family rules (carried from FerroBRIDGE and FerroTERM)

- [Owner work style](owner-work-style.md): research-first, evidence-based;
  no scaffold ahead of a decided design or its issue; pause when asked
- [Memory lives in the repo](memory-lives-in-repo.md): every learning is a
  tracked file here, never a per-user note
- [Milestones 0.0.x](milestones-0-0-x.md): start at v0.0.1, step by a patch
  number; v0.0.1 to v0.0.9 planned from the start
- [Milestone autonomy](milestone-autonomy.md): work a milestone to zero
  without asking the order, then cut the release
- [Release tag is mine](release-tag-is-mine.md): the session pushes every
  release and pre-release tag; never hand it to the owner
- [PR auto-merge](pr-auto-merge.md): arm auto-merge on open; prune merged
  branches; branch from `origin/HEAD`
- [Auto-merge follow-ups](auto-merge-follow-ups.md): never push more to an
  armed PR unless it is still BLOCKED
- [Merge queue with signed commits](merge-queue-signed.md): rebase locally,
  merge one at a time, never `gh pr update-branch --rebase`
- [PR body licence checkbox](pr-body-licence-checkbox.md): every PR body from
  the template with the licensing box ticked
- [Upstream reports stay here](upstream-reports-stay-here.md): one standing
  issue (#212), one comment per report, written to the upstream's contributing
  rules so the owner can report it back at the end
- [Upstream reports carry no milestone](upstream-reports-no-milestone.md): the
  in-repo decision is a separate, milestoned issue
- [Subagent reports go to a file](subagent-reports-to-file.md): a long agent
  report is written to the scratchpad by the agent
- [Clean up agent worktrees](clean-up-agent-worktrees.md): one at a time,
  after the agent has reported, never by loop
- [git -C and absolute paths](git-c-absolute-paths.md): a `cd` in a compound
  command does not reliably move git
- [Gates on the host](gates-on-host.md): every gate runs on the host CLI,
  never in a pinned container
- [Lint bar parity](lint-bar-parity.md): FerroEHR's strict lint table; no
  `unwrap`/`expect`/`panic` outside tests
- [File-length rule](file-length-rule.md): hand-written `.rs` at most 1000
  lines, split at 750; empty allow-list here
- [Dependency sweep to latest](deps-latest-sweep.md): compare pins with
  crates.io and FerroEHR's crate list every session
- [PostgreSQL 18](postgresql-18.md): the latest release for FerroFED's own
  optional stored-query backend; the harness nodes run FerroEHR's own
  database image (on 18.6)
- [Mermaid diagrams](mermaid-diagrams.md): render every fence with
  mermaid-cli before a PR
- [Perl edit pitfalls](perl-edit-pitfalls.md): heredoc literals, re-read
  before matching, `cargo clean -p` on a stale rmeta
- [Forum replies short and plain](forum-replies-short-plain.md): anything the
  owner posts as themself stays short, plain, and linked
- [Strict over the reference](strict-over-reference.md): the Java reference implementation is evidence, not the bar; FerroFED is as strict as FerroEHR and the specification wins where the reference is laxer; owner 2026-10-01
- [Build in FED first](build-in-fed-first.md): every capability FerroFED needs is built here behind its trait seam, as a crate that can move to FerroPIX later; never block on an unbuilt sibling; owner 2026-10-01
- [End-to-end gate](e2e-gate.md): container tests run only with FERROFED_E2E=1 through the testkit harness (two FerroEHR nodes with distinct system_ids behind capturing and fault proxies, images pinned by digest); every case seeds the subject on both nodes; 2026-10-01
- [Two FerroEHR nodes](two-ferroehr-nodes.md): CI's e2e harness runs two FerroEHR instances (plus a third for three-node cases) and the quickstart runs four, all on one PostgreSQL server with a database per node; EHRbase left because it refuses a BASE-valid namespace (not reported to EHRbase, owner 2026-10-03); decisions A44 (owner 2026-10-02) and A47 (owner 2026-10-03)
- [Repository in the FerroHEALTH org](repo-in-ferrohealth-org.md): FerroHEALTH/FerroFED since 2026-10-01, the board is org project 1; what the transfer changed and what stays owner-side (#123)
- [Cut releases promptly](cut-releases-promptly.md): cut when the milestone's code is done; move owner-side or upstream-blocked stragglers to the next milestone, never hold the cut; owner 2026-10-02
- [Oldest PR merges first](oldest-pr-merges-first.md): only the head of the merge order has auto-merge armed; newer PRs wait, so long-standing PRs stop falling behind; owner 2026-10-03
- [Native issue types and fields](native-issue-types-and-priority.md): type, priority and effort are the native issue type and the FerroHEALTH Priority and Effort fields, set with scripts/gh/fields.sh; bug, enhancement and P0 to P3 retired; owner 2026-10-02 (#154)
