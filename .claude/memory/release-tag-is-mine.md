---
name: release-tag-is-mine
description: "The session cuts every release end to end (version bump PR, merge, signed tag, push, reading the release run), pre-releases included, never handing the tag to the owner; carried from FerroBRIDGE (2026-09-13) and FerroTERM's release cadence"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-13 a pull request body told the owner which
`git tag -s` and `git push` commands to run. The owner: "never do this okay!!
... you always run the tag never say that i need to do that". FerroTERM's
owner asked the same from v0.1.3 on ("you push the tag").

**Why:** the owner wants the cut finished from the session like every other
step of a milestone; a hand-back is unfinished work.

**How to apply:** when a milestone empties, the next unit is the release:

1. A version-bump pull request that moves the version everywhere it is named
   (`Cargo.toml` once it exists, `CITATION.cff`, `docs/VERSIONS.md`, the
   README, the site, the `CHANGELOG.md` section), checked by
   `scripts/checks/versions.sh` once that guard exists.
2. Its merge, then `git tag -s vX.Y.Z -m vX.Y.Z` and `git push origin vX.Y.Z`
   from the session, then reading the release run and fixing what fails.
3. A new milestone for stragglers if needed, and the emptied milestone closed
   by hand (`gh api -X PATCH repos/FerroHEALTH/FerroFED/milestones/<n> -f
   state=closed`).

A change to the release lane is rehearsed with a `-rc.N` tag first, also
pushed from here. Docs and PR bodies never say "the owner runs" for a tag.
See [[milestone-autonomy]].
