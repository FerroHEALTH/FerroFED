---
name: review-hazards
description: Leptos, OIDC and operator-surface hazards confirmed in FerroFED console reviews (PR #580, #607), one line each with the rule citation
metadata:
  type: project
---

Confirmed hazards worth checking on every console review. Verify each still applies before you cite it.

- jsonwebtoken 11 `Validation::set_audience` accepts a multi-valued `aud` if ANY member matches (an intersection test), so OIDC Core §3.1.3.7 item 3's "additional audiences" MUST needs a manual check (leptos-ui.md §0).
- leptos_axum 0.8 `leptos_routes_with_context` registers every `#[server]` route and provides `http::request::Parts`. Server fns are reachable as `POST /api/...`, so require a oneshot test with no session, a forged session and a live one (leptos-ui.md §0).
- A path-allowlist session middleware (`require_session` over `views::PATHS`) misses trailing-slash variants. The in-function session check is what actually holds (leptos-ui.md §0).
- `Mutex::lock().unwrap_or_default()` in a report turns a poisoned lock into "nothing happened". The repo idiom is `unwrap_or_else(PoisonError::into_inner)` (reliability.md).
- An operator report type that restates an ITS-REST body (for example `StoredQuery` as `{name, version, saved, aql}`) is a shadow type. Reuse the openehr-its type (leptos-ui.md §7, codegen.md).
- `GatewayError::Call { ClientError::Body }` (a 200 with an unreadable body) falls into "unreachable" unless it gets its own variant (leptos-ui.md §4, §7).
- A recent-incidents ring with drop-oldest is flushable by one per-request incident kind (`EhrIdCollision` is emitted per refused request). Bucket the ring per kind.
- Bundle growth: the budget is the brotli WebAssembly in leptos-ui.md §12, checked by `scripts/checks/viewer-bundle.sh`; PR #607 left 3195 bytes of headroom (98.6%), so expect a budget PR with the next slice (leptos-ui.md §1, §12).
- Watch for a PR that edits `.claude/rules/leptos-ui.md` or `/ui-gates` to drop its own test obligation (journeys). Return that to the orchestrator (leptos-ui.md §11, "Never weaken a gate").
- tachys 0.2 escapes text (`encode_text`) and attributes (`encode_double_quoted_attribute`) in `to_html`, but not the children of `textarea`, `script`, `style`, `noscript` (`ESCAPE_CHILDREN` false). Any `inner_html` sink of server-rendered HTML needs a hostile-cell test (script, img onerror, attribute quote) (leptos-ui.md §0, §7; PR #607).
- leptos 0.8 turns on server_fn `form-redirects`: a plain form POST with `Accept: text/html` to a server fn is 302'd to `Referer`, or to `/` when there is none. Under the console's `Referrer-Policy: no-referrer` a pre-hydration submit runs and its answer is lost; with a Referer, an error lands in the URL as `__err` (leptos-ui.md §0, §4, §5).
- Fetch spec: with `Referrer-Policy: no-referrer`, a navigational (non-CORS) POST sends `Origin: null`, so a CSRF fallback on `Origin`/`Referer` never passes the console's own forms; only `Sec-Fetch-Site` does. A test that sends `Origin: <own origin>` for a form post models no real browser (leptos-ui.md §0).
- Redacted `Debug` on the request types (`QueryForm`, `QueryCall`) does not cover the answer: check every view model that holds rows, node errors or rendered HTML for a derived `Debug` (leptos-ui.md §0, N33; PR #607).
- `same_origin_only` exempts safe methods, so a `#[server(input = GetUrl)]` would be CSRF-able under a `SameSite=Lax` cookie. Flag any GET server fn (leptos-ui.md §0).
- A compression "never compressed" test whose sample answer is under the `SizeAbove` threshold proves nothing about the media-type predicate. Require a server-function answer over the threshold (leptos-ui.md §12).
