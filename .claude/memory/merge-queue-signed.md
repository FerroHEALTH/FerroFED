---
name: merge-queue-signed
description: "When main requires signed commits and an up-to-date branch, pull requests merge one at a time after a LOCAL signed rebase; gh pr update-branch --rebase strips the signature; carried from FerroBRIDGE (2026-09-13); FerroFED merges through the merge queue since 2026-10-05, so a branch is rebased only on a conflict"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Carried from FerroBRIDGE, where five green pull requests with auto-merge armed
sat unmerged for hours on 2026-09-13. Two `main` rules explain it: required
status checks with the strict up-to-date policy, so every merge makes the
other branches stale and auto-merge waits; and required signatures, so a
branch rewritten by `gh pr update-branch --rebase` (a server-side rebase,
which drops the author's signature) is refused even when every check is
green. FerroFED's contributing rules already require signed commits.

**Why:** GitHub's auto-merge never updates a stale branch, and it cannot sign
a rebase with the author's key.

**How to apply:** drain open pull requests as a serial queue. For each: rebase
locally onto `origin/HEAD` (signed by `commit.gpgsign`), push with
`--force-with-lease`, let `conclusion` go green and auto-merge fire, then the
next. Dependabot branches get `@dependabot rebase`. Never
`gh pr update-branch --rebase`, never `--admin` unasked. Opening a new pull
request while the queue drains makes every queued branch stale again.

**FerroFED since 2026-10-05:** `main` merges through GitHub's merge queue,
and its ruleset no longer requires an up-to-date branch (read on
2026-10-05). Arming auto-merge adds a pull request to the queue, which tests
it on top of the ones ahead of it, so the serial drain above is no longer
needed and a branch is rebased locally only when it conflicts. Signatures
are still required, so that rebase is still local and signed, and
`gh pr update-branch --rebase` is still never used.
