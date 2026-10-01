---
name: pr-body-licence-checkbox
description: "Every PR body uses .github/PULL_REQUEST_TEMPLATE.md with the licensing box ticked, or contribution-licence-guard fails; carried from FerroTERM (2026-09-22), the guard exists here since #4"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every pull request body follows `.github/PULL_REQUEST_TEMPLATE.md`, with the
line `- [x] I accept the terms in [CONTRIBUTING.md § Licensing of
contributions](...)` ticked and the checklist items that hold ticked. A free
text `gh pr create --body` fails the `contribution-licence-guard` job
(`scripts/checks/contribution-licence.sh` greps for that exact line), and the
`conclusion` check then blocks the merge.

**Why:** on FerroTERM on 2026-09-22 the owner flagged that PRs kept failing
this guard ("you are every time missing this"). FerroFED's guard landed with
#4 on 2026-09-16.

**How to apply:** write every PR body from the template, in subagent prompts
too, and keep `Closes #N` under "What changed and why", one keyword per
issue. Fix an existing PR with `gh pr edit <n> --body`. See [[pr-auto-merge]].
