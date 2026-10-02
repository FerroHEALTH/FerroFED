---
name: product-scope
description: "What FerroFED is (the owner's product statement, as the README and the repository description give it), the transparent-intermediary premise, and that the research program on #16 decided everything structural into docs/architecture.md"
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

**Decided by the research program** (#16, closed): the crate layout, the AQL
rewrite on the published `openehr-query` model, the identity binding built
first (IHE PIXm), the registry storage, the merge and completeness engine, the
stored-query registry, the outbound credential model, and the acceptance
instrument (the conformance matrix over the §17 points). The owner decided
every entry of its register on 2026-10-01, and `docs/architecture.md` is the
design of record. Build each piece from its own issue ([[owner-work-style]]),
and make no technical claim beyond the statement above, the architecture of
record, or a cited finding.

**Roadmap shape (owner, 2026-10-01):** milestones v0.0.1 to v0.0.9, with
v0.0.1 the repository setup and the research program and v0.0.2 the
workspace and the first federated query ([[milestones-0-0-x]]). v0.0.1 was
released on 2026-10-01, and v0.0.2 and v0.0.3 shipped together as release
0.0.3 on 2026-10-02.
