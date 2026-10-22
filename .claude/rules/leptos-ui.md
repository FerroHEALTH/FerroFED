---
paths: ["app/ferrofed-viewer/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos viewer rules (`app/ferrofed-viewer`, and any Leptos code)

**FerroUI leads the UI.** The console draws with `ferrohealth-ui`, pinned
exactly in the root `[workspace.dependencies]` (today `=0.2.0`), and the
FerroUI repository (FerroHEALTH/FerroUI) is the source for its components,
design, tokens, icons, accessibility bar, interaction contract, words and
quality control. Read these first, at the git tag that matches the pin
(today `v0.2.0`), and on `main` only while the console moves to the next
release:

- [`docs/consoles.md`](https://github.com/FerroHEALTH/FerroUI/blob/main/docs/consoles.md):
  the consumer guide, how a console uses the library. It is newer than
  `v0.2.0`, so read it on `main`.
- [`.claude/rules/leptos-ui.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.2.0/.claude/rules/leptos-ui.md):
  the component rules of every FerroHEALTH console. The section numbers this
  file cites (FerroUI §N) are those of the `v0.2.0` copy.
- [`crates/ferrohealth-ui/CLAUDE.md`](https://github.com/FerroHEALTH/FerroUI/blob/v0.2.0/crates/ferrohealth-ui/CLAUDE.md):
  the crate's own discipline.

The move to `ferrohealth-ui` 0.3.x, with its islands mode and its four-layer
interaction contract, is FerroUI #161 (milestone v0.4.0), which rewrites the
consoles once FerroUI 0.3.2 is out. New UI work here waits for it. FerroUI's
`design/` is the only design source; a design folder anywhere else is stale
and is never synced from. A component this console needs and the library
lacks, or a component that behaves wrongly, is an issue filed in
FerroHEALTH/FerroUI and linked from the work here: never a local copy, a
shadow component, a stylesheet override or a hand-written script in this
repository.

This file keeps only what is particular to the operator console: its routes
and pages, the BFF and its session, the server functions, the gateway client
and the admin actions, the configuration and security headers, the bundle
budget measurement, the journeys and screenshots, and the console's own
words. The console's shape is decision A55 of `docs/architecture.md`. The
oracle for every Leptos question is the official Leptos book
(<https://github.com/leptos-rs/book>, `main`, which targets Leptos 0.8)
through `/leptos-lookup`, never memory; citations below are book chapters
(`view/04_iteration`, `ssr/23_ssr_modes`) and pinned crate documentation.

**The console is rendered on the server and hydrated in the browser**, with a
backend-for-frontend (BFF) on the same server, built by cargo-leptos. Unlike
FerroEHR's viewer, the two halves are chosen by **compilation target, never by
Cargo feature**: the host build is the server half (`leptos/ssr`, axum, the
gateway client), the `wasm32-unknown-unknown` build is the browser half
(`leptos/hydrate`). The crate has no `ssr` or `hydrate` feature, so a
workspace `--all-features` build never turns both on. A rule that brings
back an `ssr`/`hydrate` feature pair is an architecture change, returned to
the orchestrator, never made in a slice.

## 0. The mandates (absolute)

- **Rust only, zero hand-written JavaScript, no inline style**: FerroUI §0.
  What this console adds: the book's `oninput="this.form.requestSubmit()"`
  trick in `router/20_form` is forbidden here (use an `on:` Rust listener),
  and the only JavaScript in the product is the `wasm-bindgen` glue and the
  hydration bootstrap the toolchain generates.
- **The console is a client of the gateway and nothing else.** It reaches the
  gateway over HTTP, on the public surface any client uses, through
  `crate::gateway` and the `openehr-its` client. It links no part of the
  gateway: not `ferrofed-engine`, not `ferrofed-identity`, not
  `ferrofed-server`. The architecture test in
  `app/ferrofed-engine/tests/it/architecture.rs` walks its dependency closure
  and fails on one. Anything the console can do, any client can do, and it
  adds no gateway behaviour. A screen that needs what the gateway does not
  expose gets a read-only operator API on the gateway first, as its own
  issue, behind the gateway's authentication.
- **The console stores no clinical data, and holds no token in the
  browser.** The query console shows the signed-in operator the rows a query
  answered (#277), and nothing keeps them: no browser storage (local or
  session storage, IndexedDB), no service worker, no cache (every page and
  every server function answer is `Cache-Control: no-store`, held by a
  test), and no URL. The operator's session is server-side
  (`crate::session`); the browser carries one opaque `HttpOnly` session
  cookie, and a second `HttpOnly` cookie that holds the operator's theme
  word (`light`, `dark` or `system`) and nothing else (`crate::theme`). An
  access token, a client secret, or a PKCE verifier never reaches a signal,
  a prop, a serialized resource, the page HTML, or a log.
  A secret the operator types for the gateway (a credential set through the
  configuration page, #578) starts in the browser: it lives only in a
  password field that is never pre-filled and in the pending server-function
  input, is held in a type that zeroes on drop and redacts its `Debug`, moves
  into the request without a copy, and is never rendered back, logged, or
  returned.
- **No patient identifier in a URL, the browser history, or a log**
  (`.claude/rules/identifier-hygiene.md`). A patient is named through a
  request body or a form the BFF reads, never a path segment or a query
  parameter the router or the browser keeps, and a test over the rendered
  output and the request log proves it for each screen that takes one.
- **Every `#[server]` function is a public HTTP endpoint**
  (`server/25_server_functions`, the security warning). It checks the session
  itself before it touches the gateway or session state, never assuming only
  this UI calls it. **Every server function is a `POST`**: the server's
  same-origin check passes safe methods, and the session cookie travels on a
  cross-site top-level `GET`, so a `GetUrl` server function would run for
  any site. `tests/it/server_functions.rs` holds every registered function
  to it.

## 1. Crate and build discipline

- One crate, two targets. Server-only dependencies live under
  `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, browser-only
  ones under `[target.'cfg(target_arch = "wasm32")'.dependencies]`, and code
  for one half only carries the matching `#[cfg(target_arch …)]`. Keep the
  lib compiling for `wasm32-unknown-unknown` at all times: the CI `viewer`
  job runs clippy on that target.
- **Before the first `#[server]` function**, confirm through `/leptos-lookup`
  and a wasm32 clippy pass that the `#[server]` macro builds under the target
  split. If it needs a crate feature named `ssr`, stop and return the finding:
  the target split is decision A55.
- **WebAssembly is 32-bit: never `usize`/`isize` in a server-function
  argument or return type, or in any serialized shared type.** Fixed-size
  integers only (`server/25` quirks).
- Signals hold `Send + Sync` values, because the server renders on a
  multi-threaded tokio runtime. The `_local` variants (`signal_local`,
  `LocalResource`, `Action::new_local`) are only for genuinely `!Send`
  browser types; a `LocalResource` where a serializable `Resource` works is a
  deoptimization under SSR (`reactivity/working_with_signals`, `ssr/23`).
- **Bundle size.** The bundle builds under the root `Cargo.toml`
  `[profile.wasm-release]` (`opt-level = "z"`, `lto = true`,
  `codegen-units = 1`), selected by `lib-profile-release` in the crate's
  `[package.metadata.leptos]`; `[profile.release]` stays the server's. Avoid
  `regex` and generics-heavy code on browser paths (factor a concrete inner
  function). A new browser-side dependency is justified against the bundle
  bytes it adds, measured from `scripts/release/viewer-site.sh --release`,
  and the bundle is held to the budget of §12.
- **The bundle name is fixed at compile time.** Leptos names the WebAssembly
  file it loads from `LEPTOS_OUTPUT_NAME`, which the repository's
  `.cargo/config.toml` sets to the crate's `output-name`, so a plain cargo
  build and a cargo-leptos build render the same page. The server test
  `the_page_loads_the_bundle_cargo_leptos_writes` holds it.
- **Views are built in `.into_any()`-erased sections.** A monolithic `view!`
  tree over deeply nested components blows rustc's layout-recursion depth at
  codegen in a plain `cargo build`, which has no `erase_components`. Break
  every screen into section functions bound to erased locals.
- The workspace bar applies unchanged (`.claude/rules/reliability.md`,
  `.claude/rules/rust-style.md`): no `unsafe`, no `unwrap`/`expect` outside
  tests, `thiserror` for the viewer's error enums, every public item
  documented, suppressions as `#[expect(lint, reason = "…")]` scoped to the
  smallest item. The `#[component]` macro drops attributes such as
  `#[must_use]` from the function it writes, so a public component carries a
  scoped `#[expect(clippy::must_use_candidate, reason = "…")]` instead.

## 2. Reactivity

The reactivity rules are FerroUI §3 (setup functions run once, the access
discipline, no signal written from an `Effect` that reads another). Beside
them (`reactivity/working_with_signals`, `15_global_state`):

- Never `sig.get().is_empty()`, which clones the whole value, and never hold
  a `.read()` guard across a write.
- A signal derived from a signal is a derived closure or a `Memo`. Effects
  only sync with the non-reactive outside world.
- Prefer local component state. Escalate in this order: the URL (router),
  then a context signal, then a `Store`. Context values use the newtype
  pattern; `expect_context` only where provision is structurally guaranteed.

## 3. Components and props

The prop rules are FerroUI §3: a changing value is a `Signal<T>` prop, and a
change reports back through a callback prop or an `on:` listener. Every
component and every prop carries a doc comment, and a `WriteSignal` goes
down only where it is needed.

## 4. Views: iteration, control flow and errors

The view rules are FerroUI §4: a `<For>` key that is stable and
data-derived, never an index; an error that never renders as nothing; valid
HTML; `rel="external"` on an anchor to a BFF axum route (the sign-in link to
`/login`). What is particular here:

- A `<For>` key of this console is an `endpoint_id`, a stored-query name and
  version, or an incident id. UI state in a list that can reorder is keyed
  by the same id, never by position.
- A gateway refusal renders its status and its stable error `code`, in the
  section whose data failed; an incomplete federated answer renders as
  incomplete, never hidden (§11.4).
- Expensive branches go behind `<Show when fallback>`; divergent branch types
  use `Either` or `.into_any()`.
- **Never create a resource inside a `Suspend` closure.** A `Suspend` re-runs
  on every notification of what it awaits, and each re-run re-creates what is
  inside it; resource ids then diverge between the server pass and
  hydration, and hydration reads the wrong serialized slot.

## 5. Forms

The form rules are FerroUI §5: a stable `id` and `name` on every control
with its label's `for`, a `<textarea>` filled through `prop:value` alone,
and no `GET` form for a field that can carry clinical content. What is
particular here:

- Controlled inputs use `prop:value` with `on:input:target`, or the
  `bind:value`/`bind:checked`/`bind:group` sugar (`view/05_forms`). The
  `value` attribute only sets the initial value.
- A form that calls a server function dispatches its action from the
  browser's own form data, the shape of `query/mod.rs`, `admin/mod.rs` and
  `configure/mod.rs`. It is a plain `<form method="post">` whose `action` is
  the server function's URL. Its `on:submit` reads
  `web_sys::FormData::new_with_form`, calls `prevent_default` and
  dispatches the typed form, and it reads each field by its `name` with
  `FormData.get`. An `<ActionForm>` would parse the URL-encoded form in the
  browser instead, about 26 KB of compressed WebAssembly (§12).
- Such a form has a plain-form fallback before hydration. Its submit button
  is disabled, and a status line says the form is ready once the page has
  loaded, until an `Effect` marks the page hydrated. A plain post that still
  arrives, before hydration or from a browser that gives no form data, is
  refused by the server function as `ViewError::PlainPost` before it asks
  the gateway anything (`query::run::server::from_the_hydrated_page`), so
  no action runs whose answer no page would show.
- `<ActionForm>` is still right for a form that must work before the bundle
  loads, whose server function answers a plain post with a redirect or a
  page of its own (`progressive_enhancement/action_form`). The pull request
  that adds the first one raises the budget in §12 by what it measures. A
  form that posts to a server route, such as the sign-out in `shell.rs`, is
  a plain `<form>` with no Leptos action.
- Parse user input yourself. `type="number"` is not validation.
- A form whose field can carry a patient identifier is a `POST` form (§0).

## 6. Async data (the BFF pattern)

- Loading is `Resource::new(source, fetcher)` calling a server function; a
  mutation is a `ServerAction` (`async/10_resources`, `async/13_actions`).
  Reactive inputs go in the source; the fetcher is untracked.
- Read resources under `<Suspense>` on first load and under `<Transition>`
  when the resource refetches on a filter, page or interval change
  (`async/12_transition`).
- Never fetch inside an `Effect` and write a signal. `spawn_local` is for
  genuine fire-and-forget only.
- Only the visible tab fetches.

## 7. The server half

- **One module owns every gateway call: `crate::gateway`.** It builds each
  request on the `openehr-its` client with the operator's token and reads the
  answer into the typed `openehr-federation` and `openehr-its` bodies. Never a
  second client, a hand-written ITS-REST route, or a `reqwest` call from a
  server function body (`.claude/rules/spec-adherence.md`).
- A server function is thin: check the session, call `crate::gateway`, map
  the error. Logic lives in plain modules with ordinary unit tests.
- Errors are typed: the gateway's `StatusCode` and error body are carried as
  data, never stringified into `ServerFnError::ServerError`, and a non-2xx
  answer is never an empty value.
- **An expected outcome is a successful server-function answer.** A server
  function returns `Result<Outcome<T>, ViewError>`: the view as
  `Outcome::Shown`, and a gateway refusal or input it cannot send as
  `Outcome::Refused(Refusal)`, with the gateway's status and code. Only the
  console's own faults are a `ViewError`, and so a `5xx`: no live session, a
  gateway it cannot reach, a body it cannot read, a broken session store.
  Chrome logs every `4xx` and `5xx` a page fetches as a console error, which
  the browser journeys fail on, and a caller branches on the two, so they
  are distinct types (`.claude/rules/reliability.md`, errors are types at a
  boundary that branches). The query console renders a refusal on the
  server into its answer (`query::answer::refused`).
- Shared state reaches a render through `leptos_routes_with_context` and
  `provide_context`, and the plain axum handlers through `Extension`
  (`server/26_extractors`).
- **The security headers are the server's**, on every answer: a
  Content-Security-Policy with a nonce minted per response and no
  `'unsafe-inline'` or `'unsafe-eval'` for scripts, `nosniff`, `DENY`
  framing, no referrer, and `no-store` on every document. A new inline
  script or style that needs the policy relaxed is the defect, not the policy.
  `form-action` names the console and, when one is configured, the origin of
  the provider's end-session endpoint and nothing else: a browser holds every
  redirect a form submission follows to it, and the sign-out form is answered
  with a redirect there (`server::form_action`).

## 8. SSR and hydration correctness (`ssr/22` to `ssr/24`)

- The app body runs on both halves, so the server HTML and the hydrated view
  must be identical. Never branch view **structure** on
  `cfg!(target_arch = "wasm32")`; a server-only side effect (setting the
  response status) is fine.
- Browser-only calls (`window`, storage, timers) go inside `Effect::new` or a
  `leptos-use` wrapper; server-only code goes in a server function or the
  BFF modules.
- Valid HTML and a deterministic first render are FerroUI §4; hydration
  walks the DOM, so a breach of either is a hydration error here.
- **A route whose content comes from the gateway renders `SsrMode::Async`**
  (`ssr/23_ssr_modes`): the server waits for every resource and sends the
  whole page, so nothing arrives in a `<template>` for an inline script to
  swap in, and the page reads with no script. `PartiallyBlocked` was
  measured to stream a blocking resource out of order on a multi-threaded
  runtime; `views::every_gateway_page_is_whole_in_the_html_the_server_sends`
  and the testkit's `console` test hold the rule.
- `leptos_meta` (`<Title>`, `<Stylesheet>`, `<Meta>`) is used from component
  bodies, never by editing the shell's `<head>` by hand (`metadata`). Every
  routed page sets a `<Title>`.

## 9. Router

- One `<Router>` at the root and `<Routes fallback=…>` with a real 404 that
  sets the status (`router/16_routes`).
- Params and queries are typed through `use_params::<T>()` and
  `use_query::<T>()` with `#[derive(Params)]`; handle the `Err` and `None`
  cases, which are user input (`router/18_params_and_queries`).
- Filter, search and pagination state lives in the URL
  (`router/20_form`), except a value that can identify a patient (§0).
- A param a same-route navigation can change is read reactively, never
  `get_untracked` at setup.

## 10. Accessibility

The bar is FerroUI §7: **WCAG 2.2 Level AA**
(<https://www.w3.org/TR/WCAG22/>) in the light and the dark theme, keyboard
operable with a visible focus, no meaning on colour alone, reduced motion
respected, a reflow at 320 px, the sticky top bar never covering the focus
(SC 2.4.11). A view uses the components and the role classes of
`ferrohealth-ui`, never a colour of its own. Checked per slice here: tables
carry real `<th scope=…>` headers, and a node status is a word as well as a
tint. The console keeps only its brand theme, `style/theme.css`, the "Azure &
Iron" accent of `assets/brand/tokens.css` over the library's roles.

`tests/it/contrast.rs` reads the role tokens of the library's stylesheet with
the brand theme laid over them, in the three theme blocks, and computes the
WCAG 2.2 contrast ratio of every text and ground pair a view can draw, at
4.5:1 for text (SC 1.4.3) and 3:1 for a control's edge and the focus ring
(SC 1.4.11). A translucent ground is laid over each opaque ground and the
worst result counts. The top bar stays in view from `md` up, so the
stylesheet's `scroll-padding-top` clears it for a focused control
(SC 2.4.11), which the test holds too.

## 11. Testing and gates

- **Business logic lives outside components**, in plain types with ordinary
  unit tests. Components stay thin.
- The HTTP surface is tested in-process: `crate::server::router` driven with
  `tower::ServiceExt::oneshot` (`tests/it/server.rs`, `tests/it/sign_in.rs`),
  and the gateway client against a `wiremock` stub gateway
  (`tests/it/gateway.rs`), the specification's own examples as the bodies.
- Gates for every viewer change (`/ui-gates`): `cargo fmt` and `leptosfmt`
  clean; `cargo clippy -p ferrofed-viewer --all-targets -- -D warnings` on
  the host and `cargo clippy -p ferrofed-viewer --lib --target
  wasm32-unknown-unknown -- -D warnings`; `cargo nextest run -p
  ferrofed-viewer`; `scripts/release/viewer-site.sh --release` completing
  when the change touches the build surface.
- **Browser journeys** drive the console in headless Chrome (#608): Rust only,
  `thirtyfour` over WebDriver through chromedriver, in the testkit's
  `tests/it/journeys/` behind `FERROFED_JOURNEYS=1`, against a running
  gateway over stub nodes, a test OpenID Provider and the console serving its
  release site bundle. Each journey fails on any error the browser logs, waits
  on elements explicitly and never on a `sleep`, and reads a form's state
  before hydration with scripts turned off. A new screen, or a new step of a
  screen, gets its journey in the same change, and the `journeys (browser)`
  CI job runs them. Playwright is JavaScript and the no-JavaScript mandate
  covers the test suite.
- **Never weaken a gate to make a change pass.** A failing wasm32 clippy pass
  usually means a dependency cannot compile for the browser, which is the gate
  working.

## 12. The bundle budget and the `wasm-release` profile

The browser downloads the WebAssembly before the page hydrates, so its size
is held to a budget in CI. After `scripts/release/viewer-site.sh --release`,
the `viewer` job runs `scripts/checks/viewer-bundle.sh`, which writes the
WebAssembly and its JavaScript glue, raw, `gzip -9` and `brotli -q 11`, to the
job summary, and fails when the brotli-compressed WebAssembly is over the
budget below. The script reads the budget from this table, so the number
changes here and nowhere else.

| Measure | Budget |
|---|---|
| WebAssembly, brotli-compressed | 512000 bytes |

- **The gating number is the brotli-compressed WebAssembly.** The book's
  `deployment/binary_size` chapter has a site serve its WebAssembly
  compressed, every current browser accepts brotli, and the compressed size
  is the download. The raw and gzip sizes are reported beside it, ungated.
- **The budget is 500 KiB.** It was set at 220 KiB (25% over the
  measured 180057 bytes of the operator views of #276, 2026-10-05,
  cargo-leptos 0.3.7, wasm-bindgen 0.2.129, Rust 1.98.1), raised to 240 KiB
  by the refusal contract of §7 (#608), and to 250 KiB by the configuration
  page (#577), which measured 246193 bytes: its focus move to the outcome
  after a refused form, which WCAG 2.2 2.4.3 asks for (§10), and the
  navigation link hidden without an admin replica. It was raised to 400 KiB
  for the redesign of #1074 (#1080), when the console measured 254426 bytes:
  the 250 KiB budget held the console to bare HTML, and the redesign's shell,
  icons, tabs and interactive query console need browser code. FerroTERM's
  viewer allows 540000 bytes gzip for the same kind of tool. It was raised
  to 432 KiB for the move to `ferrohealth-ui` (#1146), the component library
  the FerroHEALTH consoles share, which measured 417001 bytes against the
  399001 of the tree it was built on. It was raised to 500 KiB for
  `ferrohealth-ui` 0.1.7 (#1162), which measured 479645 bytes against
  0.1.6's 441357 (2026-10-19, cargo-leptos 0.3.11, wasm-bindgen 0.2.129,
  Rust 1.98.1): every word the components and the console draw comes from a
  Fluent catalog, so the bundle carries the Fluent parser, the CLDR plural
  rules and the catalogs, the cost FerroUI's gallery measured too. Server-rendered
  HTML for read-only output stays the preferred shape. The headroom takes
  ordinary growth and toolchain drift. A slice
  that needs more raises the budget in this table in its own pull request,
  with the measured size and the reason, after checking that what it adds
  belongs in the browser at all (§1).
- **The profile keeps the overflow checks.** Dropping them saves 11904 bytes
  raw and 2773 brotli-compressed (1.5%). The console's own browser code does
  no arithmetic: it renders counts and latencies the server computed. The
  checks guard the arithmetic of Leptos, serde and the standard library, where
  a wrapped length or index would draw a wrong page with no error. A 1.5%
  saving does not justify an exception to `.claude/rules/reliability.md`.
- **The profile drops the line tables** (`debug = false`). wasm-bindgen
  strips the debug sections from the bundle it writes, so the inherited
  `line-tables-only` saves the browser 431 bytes raw and 36 compressed, and
  costs a 20 MB intermediate and its build time. A browser panic still names
  its file and line: `console_error_panic_hook` prints the panic location,
  which rustc embeds as data whatever the debug setting.

| `wasm-release` variant | Raw | gzip -9 | brotli -q 11 |
|---|---:|---:|---:|
| as inherited from `release` | 620830 | 230455 | 180093 |
| without overflow checks | 608926 | 226822 | 177320 |
| without line tables (the profile) | 620399 | 230413 | 180057 |

The JavaScript glue was 22757 bytes raw and 5718 brotli-compressed in each.

- **The query console (#277) measured 224907 bytes** brotli-compressed
  (2026-10-05), 373 bytes inside the budget, so the next browser-side slice
  raises the budget in its own pull request. Its first cut measured 256392: an
  `<ActionForm>` parses the URL-encoded form in the browser (about 26 KB
  compressed), and an answer rendered by components carries its tables'
  code (about 19 KB). The form now dispatches its action from the browser's
  own form data, and the answer is rendered on the server and shown as the
  HTML it wrote. Prefer that shape for any read-only result a page shows,
  with its obligation: every `inner_html` sink of server-rendered HTML is
  built from `view!` text and attribute values alone, which Leptos escapes;
  never places data as a child of `textarea`, `script`, `style` or
  `noscript`, whose children Leptos leaves unescaped; never splices a
  `format!` string as markup; and carries a hostile-content test (markup in
  every text, a quote breaking out of every attribute) asserting the escaped
  forms are present and the raw tags absent (`query::answer` and
  `tests/it/query_safety.rs`). A form that shows its answer this way runs
  only once hydrated: its submit is disabled until an `Effect` marks the
  page loaded, and its server function refuses a plain form post.
- **The refusal contract (#608) measured 226314 bytes** brotli-compressed
  (2026-10-05, cargo-leptos 0.3.7, wasm-bindgen 0.2.129), 1407 more than the
  query console and 1034 over the 220 KiB budget, which it raised to 240 KiB.
  The bytes are the browser's decoding of `Outcome<T>` and `Refusal` for each
  of the five server functions it calls, the typed outcome §7 requires, and
  the split of the inline notices into a refusal and a fault.
- **The admin actions (#909) measured 235745 bytes** brotli-compressed
  (2026-10-15, cargo-leptos 0.3.11, wasm-bindgen 0.2.129), 9163 more than
  the 226582 of the tree they were built on. Their first cut measured
  244726: each form's fields drawn by components in the browser. The
  fields are static, so the server now renders them into the view it sends
  (`admin::fields`) and the page shows that HTML inside the one form
  section both actions share, under the `inner_html` obligation above
  (`tests/it/admin.rs`). The second action costs 1287 bytes of the total.
- **The configuration page (#577) measured 246193 bytes** brotli-compressed
  (2026-10-15, cargo-leptos 0.3.11, wasm-bindgen 0.2.129), 10448 more than
  the admin actions, which raised the budget to 250 KiB. Its forms are drawn
  on the server as the admin actions' are and posted to one server function,
  so most of the bytes are that function's typed outcome and the page's
  hydration; about 590 move focus to the outcome after a refused form
  (WCAG 2.2 2.4.3), and about 330 hide the navigation link with no replica.
- **The credentials and bindings (#578) measured 246373 bytes**
  brotli-compressed (2026-10-15, cargo-leptos 0.3.11, wasm-bindgen 0.2.129),
  180 more than the configuration page: their tables and forms, the
  confirmation and the approval are rendered on the server, so the browser
  adds the eight form fields it reads and the zeroing type that holds a
  typed secret.
- **The console's styling (#1036, #1037) measured 255004 bytes**
  brotli-compressed (2026-10-15, cargo-leptos 0.3.11, wasm-bindgen 0.2.129,
  Tailwind CSS 4.3.3). The stylesheet costs the WebAssembly nothing; the
  shell does. Its first cut, each section of the top bar and the
  navigation erased with `.into_any()`, measured 261058: an erased section
  carries its own rendering code, so the shell's shallow sections return
  their typed views. Every table scrolls sideways in its own box and
  `<main>` never does, so a reflow check of the document can fail for
  anything else (WCAG 2.2 SC 1.4.10). A box written around each table the
  browser hydrates cost 1472 bytes for the seven of the operator views; one
  section function that boxes an erased table (`views::scrolled`) brought
  the bundle to 254426 bytes, 578 below the tree without the boxes. The brand
  mark is a background the stylesheet draws, since an inline SVG brings the
  SVG rendering (§13). A tinted cell for a state word cost about 1 KB.
- **The redesign's shell (#1075) measured 303998 bytes** brotli-compressed
  (2026-10-16, cargo-leptos 0.3.11, wasm-bindgen 0.2.129, Tailwind CSS
  4.3.3), 49572 more than the 254426 of the tree it was built on. Each part
  was measured by building without it, against the 316760 of the shell with
  its pages hydrated: the Lucide icons about 5.7 KB; the router's links that
  mark the current page (`<A>`) 3481; the theme switch's `ServerAction`, its
  `Outcome<()>` and the notice of a choice not kept 3956; the gateway pill's
  rendering 1547, while its resource's server function call costs nothing
  measurable beside the decoding every resource carries; and the sign-in,
  the dashboard and the About page 13647. Those three pages are static, so
  the server now renders their markup and the page shows that HTML, as the
  admin actions' fields are shown, which saved 12762, and every link to
  them is a full load (`rel="external"`), since a client navigation would
  have no markup to show. The rest is the shell's own drawing: the top bar,
  the grouped navigation, the operator's menu, the theme state and the
  empty states. The drawer and the operator's menu are HTML popovers, so
  the browser opens and closes them and the bundle carries only the focus
  move and the close on a followed link.
- **The redesigned query console (#1078) measured 346738 bytes**
  brotli-compressed (2026-10-16, cargo-leptos 0.3.11, wasm-bindgen 0.2.129),
  about 43 KB over the shell it was built on. The answer, its summary strip,
  node chips and pages of rows stay rendered on the server; the browser adds
  the two tab lists, the paging and the focus move (about 14 KB), the editor
  and its federation options, and the parameter rows. Those rows first cost
  15 KB as a keyed `<For>` with a remove button on each; eight rows rendered
  once and shown one by one cost about 7 KB.
- **The overview (#1076) measured 323879 bytes** brotli-compressed
  (2026-10-16, cargo-leptos 0.3.11, wasm-bindgen 0.2.129, Tailwind CSS
  4.3.3), 17875 more than the 306004 of the shell it was built on. Its first
  cut, the members, the member's page and the integrity incidents drawn by
  components in the browser, measured 372721. Those three are read-only, so
  the server now renders their markup, as the dashboard's, and the page
  shows that HTML through a server function, so a navigation of the client
  router loads it and a link to the dashboard needs no full load. What the
  browser still draws is the routing table's filter, a `<For>` over the
  rows the page loaded, and the decoding of the new server functions'
  answers. Every one of those `inner_html` sinks carries its hostile-content
  test (`dashboard`, `views::members`, `views::integrity`).
- **The development banner (#1096) measured 331058 bytes**
  brotli-compressed (2026-10-16), 7179 more than the overview: the
  deployment's server function and the decoding of its `Outcome`, the
  banner and its icon, and the version in the gateway pill, all drawn in
  the shell the browser hydrates on every page.
- **The stored queries and the self-description (#1077) measured 365376
  bytes** brotli-compressed (2026-10-16, cargo-leptos 0.3.11, wasm-bindgen
  0.2.129, Tailwind CSS 4.3.3), 4042 fewer than the 369418 of the tree with
  the query console, the overview and the development banner they were
  built on. Over the query console alone they measured 348496, 1758 more
  than its 346738. The namespace cards, a query's detail with its
  highlighted AQL, and the self-description's cards and document are
  rendered on the server, and the tokenizer (`highlight`) is compiled for
  the server alone, so the browser drops the tables it drew for both pages
  and carries the detail's route, its server function's decoding, and the
  query console's reading of the stored query its URL names.
- **The configuration redesign (#1079) measured 399001 bytes**
  brotli-compressed (2026-10-16, cargo-leptos 0.3.11, wasm-bindgen 0.2.129),
  33625 more than the 365376 of the stored queries it was built on. Every tab, form,
  table and diff is rendered on the server and shown as HTML, the repeatable
  rows are copied from a `template` the server wrote, and the confirmation
  steps are HTML popovers. The browser adds the page's resource keyed by
  its path, the typed form with its nested rows and their encoding, the
  reading of every row from the form data, the row add and remove, the
  tabs' arrow keys, the line count, the focus move and the field marks of
  a refusal, and the countdown.
- **The move to `ferrohealth-ui` (#1146) measured 417001 bytes**
  brotli-compressed (2026-10-18, cargo-leptos 0.3.11, wasm-bindgen 0.2.129,
  Rust 1.98.1), 18000 more than the configuration redesign. The pages stay
  rendered on the server; the browser draws the library's shell, navigation,
  theme toggle, operator menu, filter, notices and form fields in place of
  the console's own. A twiggy reading of the unoptimised WebAssembly found
  no second version of a dependency, no debug formatting beyond the standard
  library's, and no server-only component in the browser half: the items
  that name `ferrohealth_ui` come to 29836 bytes before `wasm-opt`, and the
  rest of the growth is the console's views instantiated over the library's
  types.
- **The bundle is served compressed.** The server compresses a response
  whose media type is the bundle's (`application/wasm`, JavaScript, CSS) with
  brotli or gzip, as `Accept-Encoding` chooses, and marks it
  `Vary: Accept-Encoding`, through the `tower-http` compression layer
  (`deployment/binary_size`). A document and a server function's answer are
  never compressed: each carries what the operator entered beside data an
  attacker would want, which a compression side channel (BREACH) could
  read. `tests/it/server.rs` and `tests/it/query_safety.rs` hold both.
- **The bundle names no directory of the build host.** The panic locations
  rustc embeds as data name the source file of every crate, so an
  unremapped CI build ships `/home/runner/...` and the registry and
  toolchain paths. `scripts/release/viewer-site.sh` remaps the cargo home,
  the toolchain sysroot and the checkout to `/cargo`, `/rustc-sysroot` and
  `/ferrofed` for the WebAssembly build alone
  (`CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS` with
  `--remap-path-prefix`; `trim-paths` is unstable), and the `viewer` job's
  `scripts/checks/viewer-paths.sh` fails when a home, runner, registry or
  toolchain path appears in the WebAssembly or its JavaScript glue. A
  `RUSTFLAGS` set in the environment overrides the target variable, and the
  check then fails.
- **`panic = "unwind"`, inherited from `release`, does not apply to
  `wasm32-unknown-unknown`.** The target's standard library is built to
  abort on a panic, so a panic in the browser prints its message and
  location through `console_error_panic_hook` and stops the module; the
  `catch_unwind` that turns a server panic into a `500`
  (`.claude/rules/reliability.md`) has no browser counterpart. Keep the
  browser code free of panicking paths for that reason.

## 13. Icons

The icon rule is FerroUI §9: `ferrohealth_ui::icon::Glyph`, the Lucide
outlines the library vendors, each beside the word it illustrates, and no
icon crate of the console's own. A glyph the console needs and the library
lacks is a FerroUI issue.

## 14. Strings and locales

The catalog rule is FerroUI's (its `.claude/rules/leptos-ui.md` §13): no
string the console draws lives outside a catalog, and a language joins as a
catalog file with no code change. What is particular here:

- The console's catalog is `app/ferrofed-viewer/locales/<tag>/ferrofed-viewer.ftl`,
  embedded by `fluent_templates::static_loader!` in `crate::locale`, where
  each message is one `Msg`. A test holds every catalog directory to the
  full message set, and a throwaway catalog under `tests/locales/` shows the
  words change with no code change.
- The server chooses the locale once per request (`crate::locale::requested`):
  the best language of `Accept-Language` the console's catalog holds, else
  `[server] locale`. `App` hands the same tag to the browser through a
  `SharedValue` and provides it with `ferrohealth_ui::i18n::provide_locale`,
  so the console's words and the library's switch together, and the document
  writes it as `<html lang>`.
- `scripts/checks/stray-literals.sh app/ferrofed-viewer/src` runs in CI
  against `scripts/checks/stray-literals.baseline`: a file may hold no more
  stray literals than its count, and a change that moves strings into the
  catalog lowers the count with `--update`. Most of the remaining strings
  reach a component that takes a `&'static str`, which FerroHEALTH/FerroUI#114
  widens.
