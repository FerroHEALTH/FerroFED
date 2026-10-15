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
- A `Resource` sourced on `ServerAction::version()` refetches after every completion, refusals included; a page of server-rendered `inner_html` forms is then rebuilt, wiping what the operator typed and the focus (WCAG 2.2 3.3.7, 2.4.3; leptos-ui.md §5, §10; #577).
- A transport fault after a mutating POST (`ViewError::Unreachable`) is rendered "did not run there", but a timed-out answer may follow a completed action: say the outcome is unknown (reliability.md, leptos-ui.md §4; #909).
- A console edit form that round-trips only some fields of a registry type (`ConsoleEndpoint`) silently drops the rest on a PUT; check every field the form omits (reliability.md; #577).
- Admin-listener read types hand-written in the console (`gateway/config.rs`) drift from the server's; shared wire types belong in `ferrofed-registry` beside `console`/`operator` (leptos-ui.md §7; #577).
- Check every new screen and action for its browser journey in `tools/ferrofed-testkit/tests/it/journeys/`; #909 and #577 shipped none (leptos-ui.md §11).
- reactive_graph 0.2.15 `Action::dispatch` holds the input in `Action::input` until in-flight reaches 0 (wasm heap, never serialized). A write-only secret posted through a ServerAction belongs in a zeroizing, Debug-redacted field type, moved (not copied) into the request on the server (leptos-ui.md §0; #578).
- A token-refresh slice needs a concurrent test (join_all over one due session, token mock `.expect(1)`); sequential requests prove nothing about the per-session gate (leptos-ui.md §0, RFC 6749 §6; #643).
- Gateway `without_secrets` (revision/resources.rs) filters by key name only, so a `SecretUrl` value inside a section reaches the console page unredacted; check every rendered settings value against value-based redaction (leptos-ui.md §0; #578).
- `ConfigForm` is `#[serde(default)]` and the browser reads a missing field as "", so an absent field equals an empty one. Where empty means "remove what is held" (FHIR endpoints, #988), the add-member form or a stale page deletes held data: require a presence marker or `Option` (reliability.md; leptos-ui.md §5).
- server_fn 0.8.13 `try_into_bytes` is `body.collect()` with no limit, and the in-function session check runs after the body is read: without a `RequestBodyLimitLayer` on `/api/*`, any caller makes the console buffer an unbounded body; a size check in the fn proves nothing (leptos-ui.md §0, §12; #579).
- An `<a download>` to a route that may answer a refusal hides it: the browser saves or fails the download and never shows the status and code. `Content-Disposition: attachment` on the 200 is enough (leptos-ui.md §4; #579).
- A body-limit middleware that reads the request body itself must also bound the read in time: a chunked body sent slowly holds the task before any session check runs; wrap the read in a timeout and answer 408 (RFC 9110 §15.5.9) (#1022).
- cargo-leptos 0.3.11 (`src/ext/exe.rs`) runs a `tailwindcss` or `wasm-opt` found on PATH (`which`) first, else downloads the GitHub release archive and unpacks it with no checksum; an attested release lane needs each pre-installed with a pinned SHA-256 (ci-cd.md; #1037).
- A focusable scroll container (`<main tabindex="0">`) still needs a reflow check at 320 CSS px, not 390: WCAG 2.2 SC 1.4.10 names 320 (leptos-ui.md §10; #1037).
