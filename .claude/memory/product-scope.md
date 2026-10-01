---
name: product-scope
description: "What FerroFED is (the owner's product statement, as the README and the repository description give it), the transparent-intermediary premise, and that everything structural is research on the v0.0.1 program issue"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The product statement is the ceiling on what this repository may claim. As
the README and the GitHub repository description give it:

> An openEHR federation gateway, in pure Rust: where else the record is.
> FerroFED is a transparent ITS-REST intermediary: a client sends it an
> ordinary AQL query and never learns it was federated. The gateway resolves
> the patient first, through the index, so no directly identifying identifier
> travels in a query; then it sends standard AQL to each node, the local
> FerroEHR or a remote CDR, scoped to that node's own EHR id, and merges what
> comes back with each node's provenance. It holds no clinical data of its
> own. It follows the openEHR Federation Working Group's Federation Tier with
> AQL proposal.

**Decided by the statement:** a standalone gateway process; pure Rust; the
Federation Tier specification
(<https://github.com/syntaric/openehr-federation-spec>) as the governing
text; ITS-REST on both faces (the client face and every node); identity
resolved outside AQL; no clinical data stored at the gateway.

**Open, research on the v0.0.1 program issue** (`#16` in
`CLAUDE.md` until it is numbered): the crate layout, the AQL rewrite on the
published `openehr-query` model, the identity binding to build first (IHE
PIXm, mCSD, or the Dutch Generic Functions of Annex B), the registry storage,
the merge and completeness engine, the stored-query registry, the outbound
credential model, and the acceptance instrument (the specification's §16
tracks and §17 conformance points are the obvious candidate). Do not scaffold
ahead of it ([[owner-work-style]]), and make no technical claim beyond the
statement above or a cited finding.

**Roadmap shape (owner, 2026-10-01):** milestones v0.0.1 to v0.0.9, with
v0.0.1 the repository setup and the research program and v0.0.2 the
workspace and the first federated query ([[milestones-0-0-x]]).
