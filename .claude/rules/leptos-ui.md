---
paths: ["app/ferrofed-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos console rules (`app/ferrofed-console`, and any Leptos code)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.3.4`, with its
`split` feature), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, accessibility bar, interaction contract, words and
quality control. Read these first, at the git tag that matches the pin
(today `v0.3.4`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.4/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.4/.claude/rules/leptos-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.3.4` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.3.4/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.
- [`design/consoles/ferrofed/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.3.4/design/consoles/ferrofed):
  the reviewed design of this console. Every page matches its mockup at
  1440 and 390 px, light and dark. FerroUI's `design/` is the only design
  source; a design folder anywhere else is stale.

A component this console needs and the library lacks, or one that behaves
wrongly, is an issue filed in FerroHEALTH/FerroUI that names the existing
parts it considered, linked from the work here: never a local copy, a
shadow component, a stylesheet override or a hand-written script in this
repository. Until FerroUI ships it, the console uses what the library has.

This file keeps only what is particular to the operator console: its
routes and its forms, the BFF and its session, the gateway client and the
admin actions, the security headers, the bundle budget, the journeys and
screenshots, and the console's own words. The oracle for every Leptos
question is the official Leptos book (<https://github.com/leptos-rs/book>,
`main`, which targets Leptos 0.8) through `/leptos-lookup`, never memory.

## 0. The mandates (absolute)

- **Islands mode, the four layers.** The server renders every page whole;
  the browser hydrates only the islands FerroUI's components and
  `crate::islands` carry (the Leptos book, "Guide: Islands"). Every page
  reads, and every action posts, with no WebAssembly: native HTML first
  (links, forms, `<details>`, `<dialog>` with `command`), behaviour in Rust
  islands on top, and no hand-written JavaScript anywhere (FerroUI §0). The
  only JavaScript in the product is the hydration bootstrap and the
  `wasm-bindgen` glue the toolchain writes.
- **Two halves by target, never by feature.** The host build is the server
  half (the whole page tree, axum, the gateway client); the
  `wasm32-unknown-unknown` build is the browser half (the islands alone).
  Every page module is `#[cfg(not(target_arch = "wasm32"))]`; only
  `crate::islands` and the `manufacturer` facts compile for the browser.
  The crate has no `ssr` or `hydrate` feature.
- **The console is a client of the gateway and nothing else.** It reaches
  the gateway over HTTP, on the public surface any client uses, through
  `crate::gateway` and the `openehr-its` client, and links no part of the
  gateway (`app/ferrofed-engine/tests/it/architecture.rs`). A screen that
  needs what the gateway does not expose gets a read-only operator API on
  the gateway first, as its own issue.
- **The console stores no clinical data and holds no token in the
  browser.** Every answer is `Cache-Control: no-store`; nothing is kept in
  browser storage, a service worker or a URL. The session is server-side
  (`crate::session`): the browser carries one opaque `HttpOnly` session
  cookie and one `HttpOnly` cookie per preference (`crate::prefs`), each
  holding a word and nothing else. A secret the operator types lives only
  in a password field that is never filled, a type that zeroes on drop, and
  the request that hands it to the replica.
- **No patient identifier in a URL, the browser history, or a log**
  (`.claude/rules/identifier-hygiene.md`). A query and its parameters are a
  `POST` body; a `GET` of the query page never runs one.
- **Every state-changing request is a same-origin `POST`.** The server's
  `same_origin_only` layer refuses any other method from another origin
  (`Sec-Fetch-Site`, else `Origin`/`Referer`), and every form handler checks
  the session again before it asks the gateway or a replica.

## 1. Pages and forms

- **A page is a route of `crate::app` rendered `SsrMode::Async`**, inside
  `Suspense` with a `Suspend` that reads the gateway, so the HTML arrives
  whole: nothing waits in a `<template>` for a script
  (`tests/it/views.rs`).
- **A form posts to the page it sits on.** Its handler (`crate::act`) reads
  the fields, runs the action, and renders the same page again through
  `act::render`, with the outcome provided as context; the page draws the
  outcome at its top. A route's `POST` is registered at the same path
  pattern as the Leptos route so the two merge.
- **Rows are FerroUI's F7 rows** (`widget::repeat`): a sub-card per
  endpoint, setting or new secret (`Fields::Card`), a row of one field per
  profile or list value (`Fields::One`). Add and Remove are submit buttons
  with a name and a value (`add-parameter`, `form[edit]`); with no
  WebAssembly the page answers with the form drawn again from what was
  posted and runs nothing (`crate::configure::draft`), and with it FerroUI's
  island adds a copy of the blank row under a fresh key (`new-1`) or takes a
  row out in place, and the form posts the rows it holds. A form whose action was refused, or
  that a replica refused, is drawn again as posted, the refused field marked
  with its line and `aria-invalid`. A secret is never drawn again.
- **A confirmation is a native modal dialog.** `popover_form` and the
  console's `confirmed` part hold the reason and post `confirm`; an action
  without it asks no replica. Each dialog id is unique on its page: a table
  row's removal takes an id made from its row (`plan::removal_dialog`), and
  the item's own page keeps a removal of its own. An explanation, such as
  How configuration works, is FerroUI's `info_dialog`.
- **A secret's value field is FerroUI's masked input** (R10) with the
  hold-to-show eye, never filled.
- **Return paths are checked.** A preference post returns to the page it
  names only when that is a plain path of the console
  (`prefs::return_path`).
- Views are built in `.into_any()`-erased sections, so a deep page does not
  blow rustc's layout recursion in a plain `cargo build`.

## 2. The server half

- A handler is thin: check the session, call `crate::gateway`, map the
  answer to the page's types. Errors are types (`read::Refusal`,
  `read::Fault`), and every failed gateway call is logged once by
  `read::stopped` with what it asked for, never a body, a token or a query.
- A refusal shows its status and stable code (FerroUI's refusal R18); the
  console's own refusal of input it cannot send is an alert notice.
- The security headers (`server::with_security_headers`): a per-request CSP
  nonce on every script, `frame-ancestors 'none'`, `no-referrer`, and the
  body limit held before anything reads a body (`tests/it/body_limit.rs`).

## 3. Accessibility

The bar is FerroUI's: WCAG 2.2 Level AA in the light and the dark theme,
keyboard operable, no meaning on colour alone. The console serves FerroUI's
stylesheet and its FerroFED theme exactly as the crate ships them
(`tests/it/server.rs`), so FerroUI's contrast checks hold for every page;
the console keeps no stylesheet of its own.

## 4. Testing and gates

- Business logic lives outside components, in plain types with unit tests.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot`, the gateway and the admin listeners as
  `wiremock` stubs answering the specification's own examples
  (`tests/it/stub.rs`). Every page has a test that it answers whole, needs a
  session and escapes what the gateway names.
- Gates for every console change (`/ui-gates`): `cargo fmt`; clippy on the
  host with `--all-targets` and on `wasm32-unknown-unknown` with `--lib`,
  both at `-D warnings`; `cargo nextest run -p ferrofed-console`;
  `scripts/release/console-site.sh --release` and
  `scripts/checks/console-bundle.sh` when the change touches the browser
  half.
- **Browser journeys** drive the console in headless Chrome: Rust only,
  `thirtyfour` over chromedriver, in the testkit's `tests/it/journeys/`
  behind `FERROFED_JOURNEYS=1`. A journey fails on any error the browser
  logs and waits on elements, never on a sleep. A new screen gets its
  journey in the same change.
- **Never weaken a gate to make a change pass.**

## 5. The bundle budget

The browser downloads the islands' WebAssembly, so its size is held to a
budget in CI. After `scripts/release/console-site.sh --release`, the
`console` job runs `scripts/checks/console-bundle.sh`, which writes the
WebAssembly and its glue, raw, `gzip -9` and `brotli -q 11`, to the job
summary and fails when the brotli-compressed WebAssembly is over the budget
below. The script reads the budget from this table.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 512000 bytes |

- The gating number is the brotli-compressed WebAssembly of the entry, the
  download every page makes (the Leptos book, `deployment/binary_size`).
  The build splits (`cargo leptos build --split`) and the dependency turns
  on `ferrohealth-ui`'s `split` feature with it: FerroUI's lazy islands
  load from chunks of their own on a page that draws them, and a split
  build without the feature keeps their code in the entry
  (FerroHEALTH/FerroUI#331).
- The budget was raised step by step from 220 KiB (the operator views of
  #276) to 500 KiB (`ferrohealth-ui` 0.1.7, #1162, 479645 bytes), each step
  recorded in the pull request that took it. The islands console on
  `ferrohealth-ui` 0.3.2 (#1230) measured 315488 bytes split, 442611
  unsplit (2026-10-23, cargo-leptos 0.3.11, wasm-bindgen 0.2.129, Rust
  1.99.1): the islands FerroUI's components carry and the AQL editor. On
  `ferrohealth-ui` 0.3.3 with its `split` feature (#1233) it measured
  321296 bytes, and on 0.3.4 (#1242) 323552 bytes. A change that needs more
  raises the budget here in its own pull request, with the measured size
  and the reason.
- The `wasm-release` profile (`opt-level = "z"`, `lto = true`,
  `codegen-units = 1`, `debug = false`) keeps the overflow checks
  (`.claude/rules/reliability.md`); a panic in the browser prints its
  location through `console_error_panic_hook`.

## 6. Icons

`ferrohealth_ui::icon::Glyph`, each beside the word it illustrates, and no
icon crate of the console's own (FerroUI §9). A glyph the console needs and
the library lacks is a FerroUI issue.

## 7. Strings and locales

No string the console draws lives outside a catalog (FerroUI §13).

- The console's catalog is one file per area under
  `app/ferrofed-console/locales/<tag>/`, embedded by
  `fluent_templates::static_loader!` in `crate::locale`, each message one
  `Msg` constant in `src/locale/<area>.rs`. The loader registers FerroUI's
  number formatter, and `common.ftl` supplies `number-group` and
  `number-decimal`. A test holds every catalog directory to the full message
  set, and a throwaway catalog under `tests/locales/` shows the words change
  with no code change.
- The server chooses the locale once per request
  (`crate::locale::requested`): the operator's language preference, else
  the best language of `Accept-Language` the catalog holds, else
  `[server] locale`, provided with `ferrohealth_ui::i18n::provide_locale`
  and written as `<html lang>`. An island takes its words as props.
- `scripts/checks/stray-literals.sh` runs in CI against
  `scripts/checks/stray-literals.baseline`: a file may hold no more stray
  literals than its count, and a change that moves strings into the catalog
  lowers the count with `--update`.
