---
paths: ["app/ferrofed-viewer/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Leptos viewer rules (`app/ferrofed-viewer`, and any Leptos code)

The operator console's shape is decision A55 of `docs/architecture.md`; this
file is the enforceable discipline. The oracle for every Leptos question is
the official Leptos book (<https://github.com/leptos-rs/book>, `main`, which
targets Leptos 0.8) through `/leptos-lookup`, never memory. Citations below
are book chapters (`view/04_iteration`, `ssr/23_ssr_modes`) and pinned crate
documentation. The discipline is the family's, carried from FerroEHR's
server-rendered viewer and FerroTERM's client-rendered one, and adapted to
this crate's shape.

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

- **Rust only, zero hand-written JavaScript.** No authored `.js` files, no
  inline `<script>` bodies, no HTML `onxxx="..."` attributes carrying JS
  strings (the book's `oninput="this.form.requestSubmit()"` trick in
  `router/20_form` is forbidden here). Use an `on:` Rust listener. JS-wrapping
  crates are banned; a chart is drawn in Rust as SVG. The only JavaScript in
  the product is the `wasm-bindgen` glue and the hydration bootstrap the
  toolchain generates.
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
  (`crate::session`); the browser carries one opaque `HttpOnly` cookie. An
  access token, a client secret, or a PKCE verifier never reaches a signal,
  a prop, a serialized resource, the page HTML, or a log.
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
  any site. `tests/it/views.rs` holds every registered function to it.

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

- Component functions are **setup functions and run once**. Anything dynamic
  in a view is a signal or a closure reading signals
  (`reactivity/interlude_functions`).
- Access discipline (`reactivity/working_with_signals`): `.get()`/`.set()`
  for cheap `Copy`-ish values; `.read()`/`.write()` guards or
  `.with()`/`.update()` for collections. Never `sig.get().is_empty()`, which
  clones the whole value. Never hold a `.read()` guard across a write.
- A signal derived from a signal is a derived closure or a `Memo`. **Writing
  one signal from an `Effect` that reads another is forbidden**
  (`working_with_signals` §4). Effects only sync with the non-reactive outside
  world.
- Prefer local component state. Escalate in this order: the URL (router),
  then a context signal, then a `Store` (`15_global_state`). Context values
  use the newtype pattern; `expect_context` only where provision is
  structurally guaranteed.

## 3. Components and props

- Props that change over time are signal types (`view/03_components`): a
  reusable component takes `#[prop(into)] x: Signal<T>`.
- Doc-comment every component and every prop.
- Child to parent is a callback prop or an `on:` listener. Pass a
  `WriteSignal` down only where genuinely needed.

## 4. Views: iteration, control flow and errors

- Dynamic lists use `<For each key children>` with a **stable, unique,
  data-derived key, never an index** (`view/04_iteration`): an `endpoint_id`,
  a stored-query name and version, an incident id. UI state in a list that can
  reorder is keyed by the same id, never by position.
- Expensive branches go behind `<Show when fallback>`; divergent branch types
  use `Either` or `.into_any()`.
- **An error never renders as nothing.** Resolve a `Result` where it arrives
  and render the content or an explicit inline error view in the section whose
  data failed. A gateway refusal renders its status and its stable error
  `code`; an incomplete federated answer renders as incomplete, never hidden
  (§11.4).
- **Never create a resource inside a `Suspend` closure.** A `Suspend` re-runs
  on every notification of what it awaits, and each re-run re-creates what is
  inside it; resource ids then diverge between the server pass and
  hydration, and hydration reads the wrong serialized slot.
- **An anchor to a BFF axum route carries `rel="external"`** (the sign-in
  link to `/login`): after hydration the client router intercepts same-origin
  anchors and 404s a route it does not own.

## 5. Forms

- Controlled inputs use `prop:value` with `on:input:target`, or the
  `bind:value`/`bind:checked`/`bind:group` sugar (`view/05_forms`). The
  `value` attribute only sets the initial value.
- A mutating form is an `<ActionForm>` bound to a `ServerAction`, so it works
  before the bundle loads (`progressive_enhancement/action_form`).
- Parse user input yourself. `type="number"` is not validation.
- Give every input an explicit, stable `id` and `name`, and associate the
  label with `for`.
- A form whose field can carry a patient identifier is never a `GET` form:
  its values would land in the URL and the history (§0).

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
- Shared state reaches a render through `leptos_routes_with_context` and
  `provide_context`, and the plain axum handlers through `Extension`
  (`server/26_extractors`).
- **The security headers are the server's**, on every answer: a
  Content-Security-Policy with a nonce minted per response and no
  `'unsafe-inline'` or `'unsafe-eval'` for scripts, `nosniff`, `DENY`
  framing, no referrer, and `no-store` on every document. A new inline
  script or style that needs the policy relaxed is the defect, not the policy.

## 8. SSR and hydration correctness (`ssr/22` to `ssr/24`)

- The app body runs on both halves, so the server HTML and the hydrated view
  must be identical. Never branch view **structure** on
  `cfg!(target_arch = "wasm32")`; a server-only side effect (setting the
  response status) is fine.
- Browser-only calls (`window`, storage, timers) go inside `Effect::new` or a
  `leptos-use` wrapper; server-only code goes in a server function or the
  BFF modules.
- Views emit **valid HTML**: no block element inside `<p>`, and every
  `<table>` gets an explicit `<tbody>`. Hydration walks the DOM, so invalid
  HTML is a hydration error.
- No non-determinism in the initial render (random ids, timestamps) that
  differs between the server pass and hydration.
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

The bar is **WCAG 2.2 Level AA** (<https://www.w3.org/TR/WCAG22/>), checked
per slice. Every control is keyboard operable with a visible focus state;
tables carry real `<th scope=…>` headers; no meaning rides on colour alone,
so a node status is a word as well as a tint; contrast meets AA in the light
and the dark theme of `style/main.css`, whose colours are the "Azure & Iron"
tokens of `assets/brand/tokens.css`; motion respects
`prefers-reduced-motion`.

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
- Browser journeys are planned in #608: Rust only,
  `thirtyfour` over WebDriver, failing on any browser console error, with
  explicit waits and never a `sleep`. Playwright is JavaScript and the
  no-JavaScript mandate covers the test suite.
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
| WebAssembly, brotli-compressed | 225280 bytes |

- **The gating number is the brotli-compressed WebAssembly.** The book's
  `deployment/binary_size` chapter has a site serve its WebAssembly
  compressed, every current browser accepts brotli, and the compressed size
  is the download. The raw and gzip sizes are reported beside it, ungated.
- **The budget is 220 KiB, 25% over the measured 180057 bytes** (2026-10-05,
  the operator views of #276, cargo-leptos 0.3.7, wasm-bindgen 0.2.129, Rust
  1.98.1). The headroom takes ordinary growth and toolchain drift. A slice
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

- **Icons come from `leptos_icons` with the Lucide pack alone**:
  `icondata_lu` for the icons and `icondata_core` for the `Icon` type a prop
  names, never the `icondata` umbrella, which depends on every pack. The three
  are pinned in the root `[workspace.dependencies]` and join the crate's
  shared `[dependencies]` with the first view that draws an icon, since the
  server renders it and the browser hydrates it.
- **An icon no view names costs nothing.** Every icon of the pack is a
  `static`, and the linker drops each one nothing references. Measured on the
  profile of §12: the three crates linked with no icon used changed the
  bundle by 8 bytes raw. The first icon adds the `Icon` component and the SVG
  rendering it pulls in, 20801 bytes raw and 6292 brotli-compressed, plus 220
  bytes of JavaScript glue, once. Each further icon adds its own path data
  and its call site, about 1 KB raw and 435 bytes compressed for the second.
  The bundle then holds the path data of the icons used and no other.
- An icon never carries meaning alone (§10): it sits beside the word it
  illustrates.
