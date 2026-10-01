---
name: postgresql-18
description: "Every PostgreSQL FerroFED itself tests against or documents is the latest release (18.6 on 2026-09-12), never 16; family ruling carried from FerroBRIDGE; it applies to FerroFED's own optional stored-query backend, and a member node in the harness runs its product's documented image (EHRbase on 16.2), owner 2026-10-01"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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

**The rule governs FerroFED's own database only.** A member node in the test
harness runs its product's documented image, because the node's database is
part of the product under test: EHRbase ships `ehrbase/ehrbase-v2-postgres:16.2`,
and the harness runs exactly that, pinned by digest (`docs/architecture.md`
§13, owner decision 2026-10-01). Bootstrapping EHRbase on 18 would test a
configuration EHRbase does not ship. The exception covers a member node's own
image and nothing else.
