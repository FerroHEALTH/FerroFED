---
name: two-ferroehr-nodes
description: Owner ruling 2026-10-02 (decision A44, #155) - the e2e harness and the compose quickstart run two FerroEHR nodes; EHRbase left the topology because it refuses a BASE-valid PARTY_REF.namespace (#118)
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner ruling, 2026-10-02: "we should use two FerroEHR setups for the test
because EHRbase will not work." It supersedes decisions A40 (EHRbase's
PostgreSQL 16.2) and A41 (two products) as decision A44 in
`docs/architecture.md` §15.

**Why:** EHRbase 2.36.0 refuses a `.` in `PARTY_REF.namespace`, which BASE
`object_ref.adoc` §Attributes allows (`[a-zA-Z][a-zA-Z0-9_.:/&?=+-]*`), so an
EHR seeded on it could not carry the `urn:oid:2.999.1.<n>` issuing namespace,
and the harness had seeded EHRbase's EHRs with no subject. One of the two nodes
then could not exercise the patient carriers at all. #118 stays open as the
record of the defect; it is an `upstream-report` and is never worked around.

**How to apply:**

- Both nodes run the same pinned FerroEHR image, each on its own database
  container and with its own `system_id` (`FERROEHR__SERVER__SYSTEM_ID`): the
  harness uses `cdr-a.example.org` and `cdr-b.example.org`, the quickstart
  `node-a.quickstart.local` and `node-b.quickstart.local`, and each registry
  declares the value its node stamps.
- Every e2e case seeds `EHR_STATUS.subject` in the example arc on both nodes;
  never seed a node without a subject to suit it.
- The node-profile points (CP-18, CP-19, CP-27, #93) are scored against
  FerroEHR.
- A second product returns only when one is found that admits the BASE
  namespace, recorded as its own issue. Linked: [[e2e-gate]],
  [[postgresql-18]], [[strict-over-reference]].
