<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# How the work is organised

**Audience:** contributors.

The tracker is the worklist. Every piece of work is a GitHub issue with a
plain summary, the specification sections it answers to, and an acceptance
checklist. Nothing is tracked only in a chat or a commit message.

## Milestones are releases

Milestones run on a `0.0.x` line, starting at v0.0.1. A release is cut when its
milestone has no open issue left, and its notes are the matching section of
[`CHANGELOG.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md).
A release can carry more than one milestone. The
[releases page](https://github.com/FerroHEALTH/FerroFED/releases) carries
each release with its signed assets, and the
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
request is itself the acceptance of the contribution licence terms
([CONTRIBUTING.md](https://github.com/FerroHEALTH/FerroFED/blob/main/CONTRIBUTING.md)).
