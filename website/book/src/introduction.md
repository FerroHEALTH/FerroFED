<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroFED is a pure-Rust openEHR federation gateway, one of the
[FerroHEALTH](https://ferrohealth.eu/) family. A record held by another
organisation is out of reach today. FerroFED is meant to close that gap as a
transparent ITS-REST intermediary: a client sends it an ordinary AQL query and
never learns it was federated. The gateway resolves the patient first, outside
the query, so no directly identifying identifier travels to a node. It then
sends standard AQL to each node, scoped to that node's own EHR id, and merges
what comes back with each node's provenance. It holds no clinical data of its
own.

FerroFED follows the openEHR Federation Working Group's
[Federation Tier with AQL](https://syntaric.github.io/openehr-federation-spec/)
specification, a release candidate at v0.9.0 with a 1.0 release expected.

## Where the project is

FerroFED is in its design phase. The repository, its gates and its vendored
specifications exist; the design does not yet. It is the output of a research
program on the tracker
([#16](https://github.com/rubentalstra/FerroFED/issues/16)), which writes the
architecture of record before any code is scaffolded. There is no Cargo
workspace, no release and no binary to run. Nothing in this book describes
software you can download today, and every page says which parts are settled
and which are still open.

## How this book is organised

The four parts follow what you came to do.

- **Evaluate** answers whether FerroFED fits your problem: what the Federation
  Tier is, what FerroFED will and will not claim, the version pins, and the
  licence.
- **Operate** covers what a running gateway will need around it.
- **Integrate** covers what a client sends and what it gets back.
- **Contribute** covers how the work is tracked and which checks a change has
  to pass.

The tracker is the scope. Open issues are the worklist, milestones are
releases, and the
[roadmap](https://github.com/rubentalstra/FerroFED/milestones) is the public
view of both.
