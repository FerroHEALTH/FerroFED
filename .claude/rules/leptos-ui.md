---
paths: ["app/ferrofed-console/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos console rules (`app/ferrofed-console`, and any Leptos code)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.5.8`), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, accessibility bar, interaction contract, words and
quality control. Read these first, at the git tag that matches the pin
(today `v0.5.8`):

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.8/docs/consoles.md):
  the consumer guide, how a console uses the library.
- [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.8/.claude/rules/leptos-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.5.8` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.5.8/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.
- [`design/consoles/ferrofed/`](https://github.com/FerroHEALTH/FerroUI/tree/v0.5.8/design/consoles/ferrofed):
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

- **The whole page hydrates, and every page works with no WebAssembly.**
  The server draws the first page and the browser hydrates it whole, then
  draws every page after it, each group of pages a lazy route
  (`crate::routes`), so no link or form reloads the document (FerroUI
  `docs/consoles.md`, How a console builds). With no WebAssembly every page
  still reads and every action still posts: native HTML first (links,
  forms, `<details>`, `<dialog>` with `command`), behaviour in Rust on top,
  and no hand-written JavaScript anywhere (FerroUI §0). The only JavaScript
  in the product is the hydration bootstrap and the `wasm-bindgen` glue the
  toolchain writes.
- **Two halves by target, never by feature.** The host build is the server
  half (axum, the gateway client, the session, the server functions'
  bodies); the `wasm32-unknown-unknown` build is the browser half (the page
  tree, drawn again in the browser). The page tree, `crate::read`'s types,
  `crate::act`'s answers and `crate::configure`'s rendering compile for both;
  whatever reaches the gateway is `#[cfg(not(target_arch = "wasm32"))]`. The
  crate has no `ssr` or `hydrate` feature, and never the `leptos` feature
  `islands`. The browser half links `ferrofed-registry` (the operator reports
  and the configuration resources), `openehr-federation` and `openehr-its`
  with no feature (the self-description, the federation metadata and the
  stored query definition) of the workspace, and no other FerroFED crate.
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
  (`Sec-Fetch-Site`, else `Origin`/`Referer`) before FerroUI's answer
  wrapper reads the body, and every server function checks the session
  itself before it asks the gateway or a replica.

## 1. Pages and forms

- **A page is a route of `crate::app`, drawn by its lazy route group
  (`crate::routes`), rendered `SsrMode::Async`.** It asks what it reads of
  the gateway through a server function under a `Resource`, inside
  `Suspense` with a `Suspend`, so the server sends the HTML whole and the
  browser draws the same page from the same reads (`tests/it/views.rs`).
  What the server read of the request (the session, the operator, the
  preferences, the locale, the replicas) reaches the browser as the
  `crate::visit::Visit`, never as a cookie or the request a component reads.
- **An action is a server function behind FerroUI's `<ActionForm>`** in its
  answer mode (`widget::action`): the form draws `answer_field()`, the page
  calls `answered(&action)`, and the router is wrapped in `draw_answers`.
  Once the page has hydrated the form calls the function and the page draws
  its answer in place; with no WebAssembly the post goes to the function and
  the page it names is drawn with the answer. A function whose form has rows
  takes the fields whole (`crate::act::fields`): a field posted more than
  once is named with `[]`, and a row's field in one dotted step
  (`form[query.0.url]`), since `serde_qs` reads a step of digits as a
  sequence's place. A download (the self-description, a revision's TOML) is
  a plain route.
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
  without it asks no replica. A configuration table's removal is FerroUI's
  `danger_form_dialog`, which posts natively to the configuration's
  function until FerroUI can draw it around a server action (#1414). Each dialog id is unique on its page: a table
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
- **A resource carries what the page draws and nothing more.** Its value is
  written into the page for the browser to hydrate from, so a read the page
  draws one part of comes back reduced: the stored queries' home carries no
  AQL, and a stored query's page the AQL of the version it shows alone.
- **The query console's address keeps no key it does not read.** A `GET`
  of `/query` with any key but `stored` and `version` is sent to the same
  page without it (`server::clean_query_address`), so a parameter a link
  put in the address is never echoed into the fields that name the page.

## 2. The server half

- A server function is thin: check the session, call `crate::gateway`, map
  the answer to the page's types. Errors are types (`read::Refusal`,
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
  session and escapes what the gateway names; an action's test posts as a
  browser with no WebAssembly does, to the server function with the page in
  the answer field (`tests/it/support.rs`, `answer_post`).
- Gates for every console change (`/ui-gates`): `cargo fmt`; clippy on the
  host with `--all-targets` and on `wasm32-unknown-unknown` with `--lib`,
  both at `-D warnings`; `cargo nextest run -p ferrofed-console`;
  `scripts/release/console-site.sh --release` and
  `scripts/checks/console-bundle.sh` when the change touches the browser
  half.
- **Browser journeys** drive the console in headless Chrome: Rust only,
  `thirtyfour` over chromedriver, in the testkit's `tests/it/journeys/`
  behind `FERROFED_JOURNEYS=1`. A journey fails on any error the browser
  logs and waits on elements, never on a sleep, and on the root's
  `data-ferroui-hydrated` before it touches a hydrated page. A new screen
  gets its journey in the same change.
- **Never weaken a gate to make a change pass.**

## 5. The bundle budget

The browser downloads the page's WebAssembly, so its size is held to a
budget in CI. After `scripts/release/console-site.sh --release`, the
`console` job runs `scripts/checks/console-bundle.sh`, which writes the
WebAssembly and its glue, raw, `gzip -9` and `brotli -q 11`, to the job
summary and fails when the brotli-compressed WebAssembly is over the budget
below. The script reads the budget from this table.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 696320 bytes |

- The gating number is the brotli-compressed WebAssembly of the entry, the
  download every page makes (the Leptos book, `deployment/binary_size`).
  The build splits (`cargo leptos build --split`): each lazy route group
  loads its code from chunks of its own the first time a page of the group
  is drawn, or earlier while the browser is idle, and the site script fails
  a build whose glue imports `__wasm_split_placeholder__`, which no browser
  resolves.
- The budget is 680 KiB, a quarter over the 556958 bytes the move to
  FerroUI 0.5.7 measured (#1397, 2026-10-28, cargo-leptos 0.3.11,
  wasm-bindgen 0.2.129, wasm-opt version_123, Rust 1.99.1): whole-page
  hydration puts the renderer, the reactive graph, the router, the shell,
  the visit and the reads' serde in the entry, and every data segment
  stays there, since `--split` moves code alone. The five route groups
  measured 1025542 (Configuration, raw), 157776 (Console), 276147
  (Federation), 477569 (Overview) and 551468 (Queries, with the code
  editor), raw. Before it, the islands console on `ferrohealth-ui` 0.3.6
  measured 324245 bytes against a 500 KiB budget. A change that needs more
  raises the budget here, with the measured size and the reason.
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
  and written as `<html lang>`. The catalog is embedded in both halves, and
  the browser provides the locale the server chose (`crate::visit`), so a
  page drawn in the browser speaks as the server's does; a server function
  answers in the request's locale too.
- A handler's plain-text answer is a catalog message too, in the
  request's language (`server::plain`).
- `scripts/checks/stray-literals.sh` runs in CI with an empty baseline:
  every string a reader sees is in the catalog. It passes over the lines
  that speak to a developer or the operator's terminal (a log macro, a
  `println!` or `eprintln!` with every line of its arguments, the
  configuration's refusals and the command line's help, which stay in
  English as the log does), and each literal no reader sees as words is
  allowed with its reason in `scripts/checks/stray-literals.allow`.
- The pseudo-locale `qps` (`crate::locale::PSEUDO_TAG`) draws every word
  of the console and of FerroUI accented and two fifths longer, as
  FerroUI's gallery does. It ships no catalog directory and no request can
  choose it; `[server] locale = "qps"` serves it to a browser that asks for
  no shipped language. The `journeys::pseudo` journey holds every page to
  fitting a window 1440 and 390 px wide in it, and with
  `FERROFED_SCREENSHOTS=1` captures each under the ignored
  `target/console-screenshots/pseudo`, to be read after a layout change.
