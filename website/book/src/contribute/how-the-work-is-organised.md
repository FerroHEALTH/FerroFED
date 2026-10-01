<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# How the work is organised

The tracker is the worklist. Every piece of work is a GitHub issue with a
plain summary, the specification sections it answers to, and an acceptance
checklist. Nothing is tracked only in a chat or a commit message.

## Milestones are releases

Milestones run on a `0.0.x` line, starting at v0.0.1. A release is cut when its
milestone has no open issue left. The planned line:

| Milestone | Scope |
|---|---|
| v0.0.1 | the repository setup and the research program that writes the architecture of record |
| v0.0.2 | the Cargo workspace and the first federated query over two nodes |
| v0.0.3 | identity resolution outside AQL (§5) |
| v0.0.4 | the federated answer: completeness, timeouts, ordering, de-duplication (§9 to §11) |
| v0.0.5 | the ITS-REST surface and follow-up routing (§7a, §12) |
| v0.0.6 | targeting and self-description (§8, §7a.2) |
| v0.0.7 | definitions and membership (§12.6, §12.7, §12b) |
| v0.0.8 | security and the bindings (§13 to §15, Annex A, Annex B) |
| v0.0.9 | conformance: every conformance point scored (§16, §17) |

The [project board](https://github.com/orgs/FerroHEALTH/projects/1) shows
the same issues by status.

## Labels

Each issue carries one type label (`bug`, `enhancement`, `documentation`,
`chore`, `refactor`, `perf`, `test`, `ci`), one priority from `P0` to `P3`, and
the specification it touches: `spec:federation`, `spec:openEHR`, `spec:IHE` or
`spec:NL-GF`. Design-phase investigations carry `research`; their deliverable
is cited evidence, not code.

## Pull requests

A pull request answers one issue and says so with `Closes #<n>`. Every commit
is signed, every first-party file carries the SPDX header, and the pull
request body accepts the contribution licence terms through its checkbox
([CONTRIBUTING.md](https://github.com/FerroHEALTH/FerroFED/blob/main/CONTRIBUTING.md)).
