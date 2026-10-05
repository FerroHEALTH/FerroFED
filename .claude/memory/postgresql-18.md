---
name: postgresql-18
description: "Every PostgreSQL FerroFED itself tests against or documents is the latest release (18.6 on 2026-09-12), never 16; family ruling carried from FerroBRIDGE; it applies to FerroFED's own optional stored-query backend, and the harness nodes run FerroEHR's own database image (on 18.6), owner 2026-10-01 and 2026-10-02"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner ruling on FerroBRIDGE, 2026-09-12: the databases in tests, the container
harness, the quickstart and the documentation use the latest PostgreSQL
release, 18.6 at the time, pinned by tag and digest in `docs/VERSIONS.md`. An
older version written with no decision behind it was corrected.

**How to apply here:** the research decided it (owner, 2026-10-01,
`docs/architecture.md` §8): FerroFED needs no database for one gateway.
PostgreSQL appears only as the optional backend of the stored-query store
(`DefinitionStore`) when several gateway replicas run, and there it is the
latest release, one pin row in `docs/VERSIONS.md` added with its first
consumer, and every issue or page naming a version names that row. The
reference implementation's all-PostgreSQL registry is evidence, never a
decision.

**A member node runs its product's documented database image,** because the
node's database is part of the product under test. Both harness nodes are
FerroEHR (owner ruling 2026-10-02, decision A44, [[two-ferroehr-nodes]]), and
FerroEHR's `ghcr.io/rubentalstra/ferroehr-postgres` image is built on
`postgres:18.6`, so no node runs an older PostgreSQL. The PostgreSQL 16.2
exception for EHRbase's database (decision A40) left with EHRbase.
