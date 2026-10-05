---
name: postgresql-18
description: "Every PostgreSQL FerroFED itself tests against or documents is the latest release (18.6 on 2026-09-12), never 16; family ruling carried from FerroBRIDGE; it applies to FerroFED's own optional stored-query backend, and the harness nodes run FerroEHR's own database image (on 18.6), with EHRbase's documented 16.2 image for the node profile (#549), owner 2026-10-01 and 2026-10-02"
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
node's database is part of the product under test. Both federation nodes of
the harness are FerroEHR (owner ruling 2026-10-02, decision A44,
[[two-ferroehr-nodes]]), and FerroEHR's `ghcr.io/ferrohealth/ferroehr-postgres`
image (published there since FerroEHR 4.3.3) is built on `postgres:18.6`. The
one older PostgreSQL is EHRbase's own `ehrbase/ehrbase-v2-postgres:16.2`,
which the node profile's second CDR product runs on because it is the image
EHRbase documents beside its release (#549); `docs/VERSIONS.md` pins both.
