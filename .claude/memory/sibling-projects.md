---
name: sibling-projects
description: "The other FerroHEALTH products are servers FerroFED reaches only over their protocols: FerroEHR (../FerroEHR, the reference CDR and publisher of the openehr-* crates), FerroPIX (patient identity and the IHE profiles), FerroBRIDGE (../FerroBRIDGE, FHIR and OMOP, and the working-discipline template) and FerroTERM (../FerroTERM); all are read-only prior art, a sibling change is made in that sibling's checkout, and a tracker issue there may be filed when the owner asks"
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroFED is one of the five FerroHEALTH products; [[family-architecture]]
holds the table of what each one is and how the others reach it. The
siblings:

- **FerroEHR** at `../FerroEHR`: the openEHR CDR. It is the **reference
  node** for FerroFED, reached over ITS-REST like any other CDR, and it
  publishes the `openehr-*` crates (`openehr-query` for the AQL model,
  `openehr-its` for the ITS-REST client, `openehr-rm`, `openehr-base` and the
  rest, one lockstep line at the pin in `docs/VERSIONS.md`). The crates are
  dependencies by version from crates.io, never by path.
- **FerroPIX**: the master patient index and the IHE profiles. Patient
  identity is its lane: FerroFED reaches it as a client over IHE PIXm, PDQm
  and PMIR, through the `ihe-iti` and `nl-generic-functions` crates FerroPIX
  owns and publishes (#367), and never grows an index of its own. An
  identity capability FerroFED needs that FerroPIX lacks is filed and built
  there.
- **FerroBRIDGE** at `../FerroBRIDGE` and **FerroTERM** at `../FerroTERM`:
  the openEHR-to-FHIR-and-OMOP bridge and the FHIR terminology server. FHIR
  is FerroBRIDGE's lane, so the openEHR and FHIR exchange format (`eehrxf`)
  belongs there: it moved there in one step on 2026-10-28 (#813, A76) and
  FerroFED pins the published crate exactly. On 2026-10-01 the
  owner pointed at both as "the perfect way" to set up Claude and GitHub;
  this repository's `.claude/`, `.github/` and `scripts/` were carried from
  FerroBRIDGE, the newer of the two, and adapted.
- The shared-crate repositories (FerroAUTH, FerroAUDIT, FerroUI) carry the
  cross-cutting work FerroFED consumes as exact pins.

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
  stays theirs. A gap in a shared crate is likewise built and released in
  its owning repository, never copied here.
- No sibling is an oracle. A running FerroEHR is evidence in a test; the
  specification is the authority (`.claude/rules/spec-adherence.md`).
- Never copy code between repositories: each is its own Licensed Work, and a
  licence decision is made per repository by the owner.
- Public documents may name the siblings ([[family-naming-allowed]]).
