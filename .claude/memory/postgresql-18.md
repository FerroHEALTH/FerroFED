---
name: postgresql-18
description: "Every PostgreSQL the project tests against or documents is the latest release (18.6 on 2026-09-12), never 16; family ruling carried from FerroBRIDGE; applies if research puts the registry in PostgreSQL"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Owner ruling on FerroBRIDGE, 2026-09-12: the databases in tests, the container
harness, the quickstart and the documentation use the latest PostgreSQL
release, 18.6 at the time, pinned by tag and digest in `docs/VERSIONS.md`. An
older version written with no decision behind it was corrected.

**How to apply here:** whether FerroFED's registry (organisations, endpoints,
the `system_id` and `creating_system_id` map, the stored-query registry) uses
PostgreSQL at all is research on the v0.0.1 program; the reference
implementation uses PostgreSQL with Flyway migrations, which is evidence and
never a decision. If PostgreSQL is chosen, it is the latest release, one pin
row in `docs/VERSIONS.md`, and every issue or page naming a version names
that row.
