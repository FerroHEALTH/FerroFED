---
name: build-in-fed-first
description: "Owner 2026-10-01, scoped 2026-10-06: build the identity capabilities FerroFED needs inside FerroFED first, shaped to move to FerroPIX later, because FerroPIX has nothing built; a capability whose home is a sibling that exists (FerroBRIDGE) is built there"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
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
  and the XCPD ITI-55 adapter, shipped with the localization seam in v0.0.8
  (#85).
- The protocols live in the published, spec-named crates `ihe-iti` (a
  feature per profile), `nl-generic-functions` (a feature per function)
  and `oauth-server-metadata` (RFC 8414, which the onward grants read),
  which depend on nothing in FerroFED; the adapters onto the seams sit in
  `app/ferrofed-identity` (#106, [[published-crate-naming]]). FerroPIX can use
  `ihe-iti` directly, and a binding can move there with no change to the
  gateway core (§11). The `xcpd` feature is the only one with SOAP, HL7 v3 and
  SAML dependencies.
- The rule is for FerroPIX alone, because nothing of FerroPIX is built yet
  (owner, 2026-10-06: "that rule to build everything first in FED is only for
  the PIX because we do not have anything in PIX yet"). A capability whose
  natural home is a sibling that exists and publishes, such as FerroBRIDGE
  for the openEHR and FHIR exchange format (`eehrxf`), is built in that
  sibling and consumed here by version. This does not override
  [[openehr-crates-are-the-model]]: a gap in a crate that is published is
  still that crate's issue, never a local copy.

On 2026-10-03 the owner confirmed the order: "we do not move it yet because we
will build everything first here and then move it, because then we can go
faster in this FED project". The move is #367, deliberately unscheduled (no
milestone) until the owner calls it; nothing is moved piecemeal before then.
