---
name: owner-work-style
description: How the owner wants Ferro work done (research-first, evidence-based, from first principles, confirm foundational decisions before scaffolding, pause when asked); carried from FerroBRIDGE and FerroTERM
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Carried from FerroBRIDGE, where it is recorded with its history (2026-09-03
to 2026-09-15). The owner decides foundational architecture from **cited
research (academic papers included), not convention**, and is skeptical of
the legacy way others build this kind of software.

**Why:** the foundation is the most important thing to get right from the
start. On FerroTERM the owner stopped an early scaffold with "still in the
discover phase".

**How to apply:**

- For any foundational choice, do the research and put the evidence in front
  of the owner before building. Present options with a recommendation; do not
  default to the conventional answer.
- Do not scaffold code or a workspace while the design is open. FerroFED is
  in exactly that phase until the v0.0.1 research program closes
  ([[product-scope]]).
- Take the owner's design intuitions seriously and test them against
  evidence; reconcile rather than dismiss.
- Pure Rust, memory-safe, lightweight, single binary are standing constraints
  across the family.
- The bar (owner, 2026-09-05 on FerroBRIDGE): do your own research, find the
  best way forward, then fact-check that it is the best way against white
  papers and industry practice.
- **Pause when asked.** A pause instruction names a boundary; the work up to
  it finishes (merges, cleanup, the report) and nothing new starts past it
  until the owner writes again.
- Ask the owner when a decision is theirs; on 2026-10-01 they asked to be
  asked "a lot of questions" when unsure.
