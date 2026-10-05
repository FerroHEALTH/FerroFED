---
name: repo-in-ferrohealth-org
description: "The repository lives in the FerroHEALTH organization (FerroHEALTH/FerroFED) since 2026-10-01, with the roadmap board as org project 1; moved from rubentalstra/FerroFED by owner decision, #123"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-10-01 the owner moved FerroFED into the family organization: "we
also need to move it to this organization: https://github.com/FerroHEALTH".
The session transferred `rubentalstra/FerroFED` to `FerroHEALTH/FerroFED`
right after #106 merged (#123). The roadmap board moved too: "FerroFED
Roadmap" is project 1 under FerroHEALTH, and the user-owned board was deleted
once every item and status was on the new one.

**Why:** FerroHEALTH is the family's home, and FerroFED was the first product
repository in it; the siblings (FerroEHR, FerroBRIDGE, FerroTERM) still live
under `rubentalstra` on that date.

**How to apply:**

- Every URL names `FerroHEALTH/FerroFED`; GitHub redirects the old one, but
  nothing new is written against it. FerroFED's own image is
  `ghcr.io/ferrohealth/ferrofed`, the console's
  `ghcr.io/ferrohealth/ferrofed-viewer`, and the FerroEHR node images are
  under `ghcr.io/ferrohealth/` too since FerroEHR 4.3.3.
- The transfer turned secret scanning and push protection off (the
  organization default); they were switched back on at once. A later
  transfer checks `security_and_analysis` right after the move.
- `scripts/gh/project.sh` finds the board under the repository's owner, so it
  needed no change. The board's built-in workflows are set in the UI.
- SonarQube Cloud, the `www.ferrofed.eu` CNAME and bestpractices.dev are
  owner-side follow-ups recorded on #123.
