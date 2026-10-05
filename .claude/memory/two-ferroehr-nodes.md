---
name: two-ferroehr-nodes
description: Owner rulings 2026-10-02 (decision A44, #155) and 2026-10-03 (decision A47, #322) - the CI e2e harness runs two FerroEHR nodes (plus a third for three-node cases) and the compose quickstart runs four, all on one PostgreSQL server with a database per node; EHRbase left the topology because it refuses a BASE-valid PARTY_REF.namespace (not reported upstream: FerroFED reports to specifications, never to node products, owner 2026-10-03)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner ruling, 2026-10-02: "we should use two FerroEHR setups for the test
because EHRbase will not work." It supersedes decisions A40 (EHRbase's
PostgreSQL 16.2) and A41 (two products) as decision A44 in
`docs/architecture.md` §15. Owner ruling, 2026-10-03 (#322, decision A47,
amending A44): the quickstart runs four FerroEHR nodes, as in the FerroFED
mark, and both the quickstart and the e2e harness put every node on one
PostgreSQL server with a database per node; CI stays at two plus the third.

**Why:** EHRbase 2.36.0 refuses a `.` in `PARTY_REF.namespace`, which BASE
`object_ref.adoc` §Attributes allows (`[a-zA-Z][a-zA-Z0-9_.:/&?=+-]*`), so an
EHR seeded on it could not carry the `urn:oid:2.999.1.<n>` issuing namespace,
and the harness had seeded EHRbase's EHRs with no subject. One of the two nodes
then could not exercise the patient carriers at all. The defect is recorded
in `docs/architecture.md` decision A44 and is never worked around. It is not
reported to EHRbase: on 2026-10-03 the owner ruled that nothing is reported to
a node product, and its #212 comment is marked withdrawn. Four quickstart
nodes show what two cannot: a patient missing at some members (§11.3), a
directed query leaving the rest `excluded` (§8), and the merge over more than
two answers. One database server per topology replaced one per node, which timed
out PostgreSQL start-up under parallel e2e tests (#320).

**How to apply:**

- Every node runs the same pinned FerroEHR image with its own `system_id`
  (`FERROEHR__SERVER__SYSTEM_ID`): the harness uses `cdr-a.example.org` and
  `cdr-b.example.org`, the quickstart `node-a.quickstart.local` to
  `node-d.quickstart.local`, and each registry declares the value its node
  stamps.
- Every node connects to its own database on the one FerroEHR PostgreSQL
  server (`ferroehr_a`, `ferroehr_b`, ...), owned by a role of the same name.
  `docker/postgres/20-ferrofed-node-databases.sh` runs the image's own init
  script once per further database, then revokes `CONNECT` on each from
  `PUBLIC` and grants it to that node's role alone (the testkit's
  `a_node_role_is_refused_on_another_node_database` holds it); never copy
  FerroEHR's script, and never separate nodes by schema (FerroEHR fixes its
  schema names).
- The quickstart's patients live in `docker/quickstart/ferrofed.toml` as
  `[[dev.crossref]]` rows, and `scripts/quickstart/seed.sh` creates exactly
  those EHRs over ITS-REST: one patient at all four nodes, one at two, one at
  one, one at none.
- Every e2e case seeds `EHR_STATUS.subject` in the example arc on both nodes;
  never seed a node without a subject to suit it.
- The node-profile points (CP-18, CP-19, CP-27, #93) are scored against
  FerroEHR.
- A second product returns only when one is found that admits the BASE
  namespace, recorded as its own issue. Linked: [[e2e-gate]],
  [[postgresql-18]], [[strict-over-reference]].
