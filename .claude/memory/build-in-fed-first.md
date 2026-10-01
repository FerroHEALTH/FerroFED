---
name: build-in-fed-first
description: "Owner 2026-10-01: build every capability FerroFED needs inside FerroFED first, behind its trait seam and as a self-contained crate that can move to FerroPIX later; never block FerroFED on a sibling that is not built yet"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner, 2026-10-01, deciding the identity bindings: "maybe we should build
anything what we need to have first in the FED and then we can always move
code to PIX right otherwise we are stuck with first making PIX before making
FED".

**Why:** FerroPIX, the family's planned MPI over PIXm and PDQm, exists only as
a repository today. A gateway that waits for a sibling to be built first
cannot ship, and the identity bindings are on the gateway's critical path.

**How to apply:**

- Every identity capability FerroFED needs lands in FerroFED, behind the trait
  seams of `docs/architecture.md` §6 (`Resolver`, `Localizer`, `Directory`,
  `ConsentPrefilter`, `OnwardAuth`): the development cross-reference (#36), the
  PIXm client usable against any PIX Manager, a FerroPIX instance later (#42),
  and the XCPD ITI-55 adapter, scheduled with the localization seam in v0.0.8
  (#85).
- Each binding is its own crate (`ferrofed-identity-ihe`,
  `ferrofed-identity-xcpd`, `ferrofed-identity-nl`), depending only on the
  traits in `ferrofed-identity` and its own protocol stack, so it can move to
  FerroPIX later with no change to the gateway core (§11). The XCPD crate is
  the only one with SOAP, HL7 v3 and SAML dependencies.
- The same holds for any sibling that does not exist yet: build the piece here
  behind a seam, shaped to move, and record the move as a later issue. This
  does not override [[openehr-crates-are-the-model]]: a gap in a crate that is
  published is still a FerroEHR issue, never a local copy.
