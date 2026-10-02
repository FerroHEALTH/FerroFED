---
name: sibling-projects
description: "FerroEHR (../FerroEHR, the reference CDR and publisher of the openehr-* crates), FerroTERM (../FerroTERM) and FerroBRIDGE (../FerroBRIDGE, the working-discipline template) are read-only prior art from FerroFED; a sibling change is made in that sibling's checkout on its own branch, and a tracker issue there may be filed when the owner asks"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroFED is one of the FerroHEALTH family (the family repository is
`../FerroHEALTH`; its README lists the products and what calls what). The
siblings checked out beside it on the owner's machine:

- **FerroEHR** at `../FerroEHR`: the openEHR CDR. It is the **reference
  node** for FerroFED, reached over ITS-REST like any other CDR, and it
  publishes the `openehr-*` crates (`openehr-query` for the AQL model,
  `openehr-its` for the ITS-REST client, `openehr-rm`, `openehr-base` and the
  rest, one lockstep line at the pin in `docs/VERSIONS.md`). The crates are
  dependencies by version from crates.io, never by path.
- **FerroBRIDGE** at `../FerroBRIDGE` and **FerroTERM** at `../FerroTERM`:
  the openEHR-to-FHIR-and-OMOP bridge and the FHIR terminology server. On
  2026-10-01 the owner pointed at both as "the perfect way" to set up Claude
  and GitHub; this repository's `.claude/`, `.github/` and `scripts/` were
  carried from FerroBRIDGE, the newer of the two, and adapted.

**How to apply:**

- Read any sibling freely: code, rules, history, closed issues. That is the
  fastest source of prior art for a decision here.
- **Never edit a sibling from this repository on your own initiative.** When
  the owner directs a sibling change from a FerroFED session, it is done in
  that sibling's checkout on its own branch and pull request, under that
  sibling's `CLAUDE.md`, rules and memory, never mixed into a FerroFED branch.
  Each repository's owner rules outrank a brief written from another: FerroEHR
  works in one checkout with no git worktrees (its own memory), so a brief for
  FerroEHR says so.
- **A tracker issue in a sibling may be filed from here when the owner asks**
  (a FerroBRIDGE ruling of 2026-09-05), and **a gap in an `openehr-*` crate is
  always filed in FerroEHR's tracker** (owner, 2026-10-01,
  [[openehr-crates-are-the-model]]): the request, such as a public reader on
  `openehr-query` for the AQL rewrite, goes in with its labels, and is
  recorded on the FerroFED issue that depends on it. Issues only; the code
  stays theirs.
- No sibling is an oracle. A running FerroEHR is evidence in a test; the
  specification is the authority (`.claude/rules/spec-adherence.md`).
- Never copy code between repositories: each is its own Licensed Work, and a
  licence decision is made per repository by the owner.
- Public documents may name the siblings ([[family-naming-allowed]]).
