---
name: license-busl
description: "FerroFED's own code and text are under the Business Source License 1.1 with Cadasto B.V. as Licensor (#1, #2, 2026-09-16; holder changed by #602, 2026-10-05) and the contribution-licence terms, checkbox and guard (#3, #4); Apache 2.0 is the Change License four years after each version"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The repository opened under the Business Source License 1.1, the family's
terms. On 2026-09-16 issue #1 (PR #2) named the Licensor and copyright
holder, which the owner changed to Cadasto B.V. on 2026-10-05 (#602), and issue #3 (PR #4) added the contribution-licence
terms in CONTRIBUTING.md, the pull-request checkbox, and the
`contribution-licence-guard` workflow backed by
`scripts/checks/contribution-licence.sh`.

The terms, as `LICENSE` and `NOTICE` state them:

- Free to read, build, modify, and redistribute.
- Free for every non-production use and for non-commercial production use.
- A commercial licence from the Licensor for any other production use.
- The Change License is Apache License 2.0, four years after each version.
- A contribution is licensed under the same licence and grants the Licensor a
  relicensing right; the contributor keeps their copyright, and there is no
  separate agreement to sign.

**How to apply:**

- Every first-party file carries `SPDX-FileCopyrightText: Cadasto
  B.V.`, which `scripts/checks/copyright-holder.sh` enforces, and `SPDX-License-Identifier: BUSL-1.1` in its header.
- `LICENSE` is the one file that names Apache 2.0 as a licence of its own,
  where it is the Change License.
- Vendored material keeps its upstream terms (the specification is CC0 1.0,
  the reference implementation Apache-2.0), recorded in each `PROVENANCE.md`.
- Every PR body ticks the licensing box ([[pr-body-licence-checkbox]]).
- The licence is decided per repository by the owner and never assumed from a
  sibling ([[sibling-projects]]).
