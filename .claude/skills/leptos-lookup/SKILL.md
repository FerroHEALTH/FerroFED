---
name: leptos-lookup
description: >
  Finds and reads the authoritative Leptos guidance for a topic (signals,
  effects, <For>, Resource, Suspense and Transition, server functions,
  hydration, router, forms, cargo-leptos, binary size) in the official Leptos
  book, cached locally. Use before implementing or reviewing any operator
  console (app/ferrofed-viewer) behaviour the rule file does not fully
  settle, or when a "how does Leptos do X" question comes up.
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "<signal / resource / server fn / hydration / router / form / cargo-leptos topic>"
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# /leptos-lookup

Answer Leptos questions from the official book text, never from memory.
Leptos is pre-1.0 and moves, so training data drifts. The distilled rules live
in `.claude/rules/leptos-ui.md`; this skill goes back to the source when a
case is not covered or needs full context.

**The console is rendered on the server and hydrated in the browser**, built
by cargo-leptos, with its two halves chosen by compilation target rather than
by the `ssr`/`hydrate` features the book's examples use. Read the SSR,
hydration, server-function and cargo-leptos chapters as written, and when an
answer depends on a crate feature named `ssr` or `hydrate`, say so
explicitly: under this crate's target split it may not apply as the book
states it.

## Procedure

1. **Ensure the book cache exists** (shared, survives sessions):

   ```bash
   BOOK=~/.cache/ferrofed/leptos-book
   [ -d "$BOOK/src" ] || git clone --depth 1 https://github.com/leptos-rs/book "$BOOK"
   ```

   If the cache is older than about 30 days
   (`git -C "$BOOK" log -1 --format=%cr`), `git -C "$BOOK" pull --ff-only`
   first. The book's `main` targets the current Leptos 0.x line, so
   cross-check any version-sensitive answer against the `leptos` pin in
   `docs/VERSIONS.md`.

2. **Route to the owning chapter** (`src/SUMMARY.md` is the index):
   - signals, effects, memos, derived values: `src/reactivity/*`, plus the
     appendices `appendix_reactive_graph.md`, `appendix_life_cycle.md`
   - components, props, children, context: `src/view/03…09_*`,
     `src/interlude_projecting_children.md`
   - lists and keys: `src/view/04_iteration.md`, `04b_iteration.md`
   - control flow and errors: `src/view/06…07_*`
   - forms: `src/view/05_forms.md`, `src/router/20_form.md`,
     `src/progressive_enhancement/action_form.md`
   - async, `Resource`, `Suspense`, `Transition`, `Action`: `src/async/*`
     (`10_resources.md` on the `Resource` and `LocalResource` split)
   - cargo-leptos, the SSR life cycle, rendering modes, hydration bugs:
     `src/ssr/21…24_*`
   - server functions, extractors, responses: `src/server/25…28_*`
   - router, params, queries: `src/router/*`; global state:
     `src/15_global_state.md`
   - WebAssembly size and deployment: `src/deployment/*`
   - styling: `src/interlude_styling.md`; head and metadata:
     `src/metadata.md`; testing: `src/testing.md`; `web_sys` and JS interop:
     `src/web_sys.md`; islands: `src/islands.md`

3. **Grep** the exact API name (`Resource`, `Suspend`, `ServerAction`,
   `bind:value`, `use_query`, `HydrationScripts`) across `src/**/*.md` when the
   routing is not obvious.

4. **Read the surrounding section**, including the warning blocks, because the
   book's prohibitions live there.

5. **Answer with citations** (`<chapter>.md` plus the heading). For API
   signatures beyond the book, follow its docs.rs links rather than guessing;
   Context7 is the preferred fetcher for a pinned crate's documentation.
   cargo-leptos configuration questions go to its README
   (<https://github.com/leptos-rs/cargo-leptos>). If the answer belongs in the
   standing rules, propose the `.claude/rules/leptos-ui.md` addition
   explicitly.
