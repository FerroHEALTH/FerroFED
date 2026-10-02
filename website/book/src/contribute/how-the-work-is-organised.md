<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# How the work is organised

The tracker is the worklist. Every piece of work is a GitHub issue with a
plain summary, the specification sections it answers to, and an acceptance
checklist. Nothing is tracked only in a chat or a commit message.

## Milestones are releases

Milestones run on a `0.0.x` line, starting at v0.0.1. A release is cut when its
milestone has no open issue left, and its notes are the matching section of
[`CHANGELOG.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md).
The line:

| Milestone | Scope | Release |
|---|---|---|
| v0.0.1 | the repository setup and the architecture of record | 0.0.1, 2026-10-01 |
| v0.0.2 | the Cargo workspace and the first federated query over two nodes | 0.0.3, 2026-10-02 |
| v0.0.3 | identity resolution outside AQL (§5) | 0.0.3, 2026-10-02 |
| v0.0.4 | the federated answer: completeness, timeouts, ordering, de-duplication (§9 to §11) | open |
| v0.0.5 | the ITS-REST surface and follow-up routing (§7a, §12) | open |
| v0.0.6 | targeting and self-description (§8, §7a.2) | open |
| v0.0.7 | definitions and membership (§12.6, §12.7, §12b) | open |
| v0.0.8 | security and the bindings (§13 to §15, Annex A, Annex B) | open |
| v0.0.9 | conformance: every conformance point scored (§16, §17) | open |

v0.0.2 and v0.0.3 shipped together as release 0.0.3, and no final 0.0.2 was
tagged. The [releases page](https://github.com/FerroHEALTH/FerroFED/releases) carries each
release with its signed assets, and the
[project board](https://github.com/orgs/FerroHEALTH/projects/1) shows the same
issues by status.

## Type, priority and labels

Each issue carries GitHub's issue type (`Bug`, `Feature` or `Task`) and the
organisation's `Priority` and `Effort` fields. A `Task` also carries one
work-kind label (`documentation`, `chore`, `refactor`, `perf`, `test` or `ci`).
Labels name the specification an issue touches: `spec:federation`,
`spec:openEHR`, `spec:IHE` or `spec:NL-GF`. The research issues that wrote the
architecture of record carry `research`; their deliverable was cited evidence
on the issue thread.

## Pull requests

A pull request answers one issue and says so with `Closes #<n>`. Every commit
is signed, every first-party file carries the SPDX header, and the pull
request body accepts the contribution licence terms through its checkbox
([CONTRIBUTING.md](https://github.com/FerroHEALTH/FerroFED/blob/main/CONTRIBUTING.md)).
