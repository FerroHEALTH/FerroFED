<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Memory index

- [Family architecture](family-architecture.md): five separate servers that
  reach each other only over published protocols, shared crates for
  cross-cutting work, nothing hand-written that a shared crate covers,
  server-rendered consoles; FerroFORMS and Veredictum out of scope, the umbrella active again;
  owner 2026-10-25

## FerroFED decisions

- [Product scope](product-scope.md): the owner's product statement is the
  ceiling on what this repository may claim; a transparent ITS-REST gateway
  on the Federation Tier specification; everything structural was decided by
  the v0.0.1 research program into `docs/architecture.md`
- [Spec pin 1.0.0](spec-pin-1-0-0.md): specification at its 1.0.1 erratum,
  untagged commit `b4242c2` by the owner's exception of 2026-11-02 (#1478,
  retired by #1480), after tag `1.0.0` (`7dcf0f4`, #17); reference
  implementation at `9cb8606`
- [Crate split](crate-split.md): spec-derived crates split from the app crates
  as in every product; nothing published for now, publishing is a one-line
  `publish` switch with the whole lane built; owner 2026-10-01
- [Published crate naming](published-crate-naming.md): a publishable crate
  carries its specification's name, never ferrofed-*; one crate per
  specification with a feature per layer or profile; FED glue under app/;
  names claimed with 0.0.0 placeholders;
  owner 2026-10-01
- [Family naming allowed](family-naming-allowed.md): public documents may name
  FerroHEALTH and the siblings, unlike FerroBRIDGE; owner 2026-10-01
- [One setup PR](one-setup-pr.md): the opening setup lands as one large pull
  request, then one PR per issue resumes; owner 2026-10-01
- [Licence: BUSL 1.1](license-busl.md): Cadasto B.V. is the Licensor
  (#1, #2) and the commercial licence contact (#616), and the contribution
  terms, checkbox and guard landed (#3, #4)
- [Domain ferrofed.eu](domain-ferrofed-eu.md): a Pages setting on the family
  model, never a `CNAME` file
- [Sibling projects](sibling-projects.md): FerroEHR is the reference node and
  publishes the `openehr-*` crates; FerroPIX serves patient identity, reached
  as a client; FerroBRIDGE owns FHIR (`eehrxf` moves there) and with
  FerroTERM is the working-discipline template; all read-only from here
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
  number; v0.0.1 to v0.0.9 planned from the start, v0.0.10 and v0.0.11 added since
- [Milestone autonomy](milestone-autonomy.md): work a milestone to zero
  without asking the order, then cut the release
- [Release tag is mine](release-tag-is-mine.md): the session pushes every
  release and pre-release tag; never hand it to the owner; it also merges the release PR and approves crates-io
- [PR auto-merge](pr-auto-merge.md): arm auto-merge on open; prune merged
  branches; branch from `origin/HEAD`
- [Auto-merge follow-ups](auto-merge-follow-ups.md): never push more to an
  armed PR unless it is still BLOCKED
- [Merge queue with signed commits](merge-queue-signed.md): a conflict takes a
  local signed merge, never `gh pr update-branch --rebase`; no commit-message
  line starts with "-" or a git header word (2026-11-09)
- [Upstream reports stay here](upstream-reports-stay-here.md): one standing
  issue (#212), one comment per report, written to the upstream's contributing
  rules: the problem and a proposed resolution with its rationale, never how
  FerroFED does it
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
  database image (on 18.6), EHRbase's node profile its documented 16.2 image
- [Mermaid diagrams](mermaid-diagrams.md): render every fence with
  mermaid-cli before a PR
- [Perl edit pitfalls](perl-edit-pitfalls.md): heredoc literals, re-read
  before matching, `cargo clean -p` on a stale rmeta
- [Forum replies short and plain](forum-replies-short-plain.md): anything the
  owner posts as themself stays short, plain, and linked
- [Strict over the reference](strict-over-reference.md): the Java reference implementation is evidence, not the bar; FerroFED is as strict as FerroEHR and the specification wins where the reference is laxer; owner 2026-10-01
- [End-to-end gate](e2e-gate.md): container tests run only with FERROFED_E2E=1 through the testkit harness (two FerroEHR nodes with distinct system_ids behind capturing and fault proxies, EHRbase for the node profile, the reference implementation for the differential run, images pinned by digest); every case seeds the subject on both nodes; 2026-10-01
- [Two FerroEHR nodes](two-ferroehr-nodes.md): CI's e2e harness runs two FerroEHR instances (plus a third for three-node cases) and the quickstart runs four, all on one PostgreSQL server with a database per node; EHRbase left the federation topology because it refuses a BASE-valid namespace (not reported to EHRbase, owner 2026-10-03) and returned only for the node profile (#549); decisions A44 (owner 2026-10-02) and A47 (owner 2026-10-03)
- [Repository in the FerroHEALTH org](repo-in-ferrohealth-org.md): FerroHEALTH/FerroFED since 2026-10-01, the board is org project 1; what the transfer changed and what stays owner-side (#123)
- [Cut releases promptly](cut-releases-promptly.md): cut when the milestone's code is done; move owner-side or upstream-blocked stragglers to the next milestone, never hold the cut; owner 2026-10-02
- [No personal data in requests](no-personal-data-in-requests.md): no worker sends the owner's email or contact to an external service, User-Agent included; a neutral agent string, and a slip is reported
- [Native issue types and fields](native-issue-types-and-priority.md): type, priority and effort are the native issue type and the FerroHEALTH Priority and Effort fields, set with scripts/gh/fields.sh; bug, enhancement and P0 to P3 retired; owner 2026-10-02 (#154)
- [FerroUI](ferroui.md): every console draws in FerroUI; uniform design, shared components, server-rendered since FerroUI 0.6, every console moves in lockstep with each FerroUI release; owner 2026-10-18
- [Release closes milestone](release-closes-milestone.md): release.yml refuses a tag whose milestone is missing or not empty and closes it after every leg; check it after a manual or recovery release; owner 2026-10-19 (#1170)
- [Helm and Kubernetes in Podman](helm-kubernetes-in-podman.md): owner 2026-10-25, test charts locally on kind over Podman; install helm when missing; never build images locally
- [Reactive console UI](reactive-console-ui.md): owner 2026-10-29 (FerroUI #510), server-rendered consoles on FerroUI 0.6: in-page state kept by a module, a navigation one document; supersedes the 2026-10-25 client-navigation ruling

## Family rules (carried in every FerroHEALTH repository, 2026-11-09)

- [No compatibility shims](no-compat-shims.md): refactors break APIs freely and leave no deprecated item, alias, re-export or stub; a removal refusal lives ONE release, then is deleted fully; no legacy support ever; owner 2026-11-01, 2026-11-09
- [No half solutions](no-half-solutions.md): build what the issue asks the proper way and validate the need against its consumer first; a breaking change moves the minor version, never shape a design to dodge semver; owner 2026-11-08
- [EHDS above openEHR](ehds-above-openehr.md): where Regulation (EU) 2025/327 conflicts with a standard's text the Regulation wins; never revert EHDS behaviour to pass a conformance row; owner 2026-11-09
- [Terminology via the server](terminology-via-server.md): every production value map goes through a FHIR terminology server over ferrobridge-term; embedded or offline maps are test-only; owner 2026-11-09
- [Workers on Opus only](workers-on-opus-only.md): every subagent is model opus; never Fable, Sonnet or Haiku as a worker; owner 2026-10-30
- [Ten worker slots](worker-cap-ten.md): up to 10 implementation workers at once across the family since 2026-11-03, replacing the cap of 5
- [No workflows for workers](no-workflows-for-workers.md): start workers with the Agent tool only, since a workflow agent cannot be messaged; owner 2026-11-01
- [Worker scratch subfolders](worker-scratch-subfolders.md): scratch files under the worker's own scratchpad subfolder, never the root; check a file is yours before --body-file and re-read the PR body; 2026-10-20
- [Worker wait loops](worker-wait-loops.md): no self-matching pgrep waits, one wait per output, push a finished branch, stop a process by PID and never pkill; 2026-10-16
- [Workers never idle on the merge queue](workers-never-idle-on-merge-queue.md): start the next independent issue in a fresh worktree while a PR waits in the queue; owner 2026-11-03
- [Workers own every open PR](workers-own-every-open-pr.md): drive every PR you opened to merge; the orchestrator sweeps every open PR after each merge, red main first; owner 2026-11-01, 2026-11-03
- [PR backlog discipline](pr-backlog-discipline.md): several open PRs are fine if each merges cleanly; disjoint files, no stacked PRs, every PR tracked to its merge; owner 2026-10-17
- [Worktrees in the repo](worktrees-in-repo.md): every worktree at <repo>/.claude/worktrees/<name>, never a sibling folder or the scratchpad; owner 2026-10-21
- [Admin bypass stays](admin-bypass-stays.md): the org-admin bypass on main stays in every family repo; nobody merges through it; owner 2026-10-21
- [Decide, do not ask](decide-dont-ask.md): make tracker, design and release calls yourself and act; ask the owner only when a tool is refused; owner 2026-11-03
- [Autonomous loop](autonomous-loop.md): never stop and never wait idle; keep working the issues while PRs drain; owner 2026-10-17
- [Always a next milestone](next-milestone-always.md): every repo keeps a milestone with a due date after the active one; owner 2026-11-03
- [One milestone at a time](one-milestone-at-a-time.md): every worker on the current milestone until it is cut; en-route issues join it and are fixed first; owner 2026-10-19
- [Shared-crate blockers](shared-crate-blockers.md): every filed issue gets a milestone; a shared-crate defect goes in the owner repo's current milestone with a worker at once; no consumer workaround; owner 2026-11-07
- [Family toolchain](family-toolchain.md): newest stable Rust as toolchain, the toolchain minus one as MSRV; never lower an MSRV; owner 2026-10-21
