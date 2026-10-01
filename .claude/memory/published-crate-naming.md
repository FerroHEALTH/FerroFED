---
name: published-crate-naming
description: "Owner 2026-10-01: a crate that may be published carries the name of the specification it implements, never ferrofed-*; one crate per specification with a feature per layer or profile; FerroFED's own glue lives under app/; the three names were claimed with 0.0.0 placeholders"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner, 2026-10-01: "for crates that we will publish never name it
ferrofed-* because that's stupid call it the official name of the crate
itself".

**Why:** a published crate is found by the specification a developer is
implementing, and the family already names its crates that way
(`openehr-query`, `openehr-its`, `fhir-types`). A product prefix tells a
reader who wrote the crate, not what it does, and it ties a reusable crate to
one product: FerroPIX should be able to use the IHE clients without depending
on anything called FerroFED.

**How to apply:**

- A crate that may be published lives under `crates/` and carries the name of
  the specification it implements: `openehr-federation` (the Federation Tier
  with AQL), `ihe-iti` (the IHE IT Infrastructure profiles),
  `nl-generic-functions` (the Dutch Generic Functions).
- One crate per specification, with a feature per layer or profile
  (`openehr-federation`: `aql`, `merge`; `ihe-iti`: `pixm`, `pdqm`, `mcsd`,
  `pmir`, `xcpd`; `nl-generic-functions`: `nvi`, `mitz`, `lrza`, `nuts-auth`),
  so a caller compiles only what it uses. A new crate is the exception, for a
  new specification.
- A published crate depends on nothing in FerroFED. FerroFED's own glue (the
  registry, the identity traits and their adapters, the engine, the server)
  lives under `app/` as `ferrofed-*` with a hard `publish = false`.
- Before a name is used, it is claimed on crates.io with a 0.0.0 placeholder
  that holds the name and contains no API (the three above, published from
  outside the workspace on 2026-10-01), and the crate's line in the workspace
  starts at 0.0.1. Publishing stays behind the workspace switch
  ([[crate-split]]).
- The layout landed with #106; `docs/architecture.md` §11 and decision A34 are
  the design of record.
