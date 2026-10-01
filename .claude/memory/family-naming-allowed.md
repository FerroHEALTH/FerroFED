---
name: family-naming-allowed
description: "Owner ruling 2026-10-01: FerroFED's public documents may name FerroHEALTH and the sibling products (the README already names FerroEHR); FerroBRIDGE's never-name-a-sibling rule does not apply here"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroBRIDGE carries a standalone-product rule (owner, 2026-09-04): its public
documents never name FerroEHR or FerroTERM. Asked on 2026-10-01 which rule
applies to FerroFED, whose README already names FerroHEALTH and "the local
FerroEHR", the owner chose family naming.

**Why:** FerroFED was opened as one of the FerroHEALTH family, and its README
places it in that family from the first commit.

**How to apply:** the README, the site and `docs/` may name the family and the
sibling products where that helps the reader. Two things stay true and are
said plainly where it matters: the gateway works against any openEHR CDR over
ITS-REST, and no sibling is a compile-time dependency beyond the published
`openehr-*` crates. Never cite a sibling as the authority for a behaviour; the
specification is ([[sibling-projects]]). Do not port FerroBRIDGE's
`standalone-product` memory here.
