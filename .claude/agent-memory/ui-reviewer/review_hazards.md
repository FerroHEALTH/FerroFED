---
name: review-hazards
description: Console, OIDC and operator-surface hazards confirmed in FerroFED console reviews (PR #580, #607, #1457), one line each with the rule citation
metadata:
  type: project
---

Confirmed hazards worth checking on every console review. Verify each still applies before you cite it.

- jsonwebtoken 11 `Validation::set_audience` accepts a multi-valued `aud` if ANY member matches (an intersection test), so OIDC Core §3.1.3.7 item 3's "additional audiences" MUST needs a manual check (console-ui.md §0).
- A path-allowlist session middleware (`require_session` over `views::PATHS`) misses trailing-slash variants. The in-function session check is what actually holds (console-ui.md §0).
- `Mutex::lock().unwrap_or_default()` in a report turns a poisoned lock into "nothing happened". The repo idiom is `unwrap_or_else(PoisonError::into_inner)` (reliability.md).
- An operator report type that restates an ITS-REST body (for example `StoredQuery` as `{name, version, saved, aql}`) is a shadow type. Reuse the openehr-its type (console-ui.md §7, codegen.md).
- `GatewayError::Call { ClientError::Body }` (a 200 with an unreadable body) falls into "unreachable" unless it gets its own variant (console-ui.md §4, §7).
- A recent-incidents ring with drop-oldest is flushable by one per-request incident kind (`EhrIdCollision` is emitted per refused request). Bucket the ring per kind.
- Watch for a PR that edits `.claude/rules/console-ui.md` or `/ui-gates` to drop its own test obligation (journeys). Return that to the orchestrator (console-ui.md §11, "Never weaken a gate").
- Fetch spec: with `Referrer-Policy: no-referrer`, a navigational (non-CORS) POST sends `Origin: null`, so a CSRF fallback on `Origin`/`Referer` never passes the console's own forms; only `Sec-Fetch-Site` does. A test that sends `Origin: <own origin>` for a form post models no real browser (console-ui.md §0).
- Redacted `Debug` on the request types (`QueryForm`, `QueryCall`) does not cover the answer: check every view model that holds rows, node errors or rendered HTML for a derived `Debug` (console-ui.md §0, N33; PR #607).
- A transport fault after a mutating POST (`ViewError::Unreachable`) is rendered "did not run there", but a timed-out answer may follow a completed action: say the outcome is unknown (reliability.md, console-ui.md §4; #909).
- A console edit form that round-trips only some fields of a registry type (`ConsoleEndpoint`) silently drops the rest on a PUT; check every field the form omits (reliability.md; #577).
- Admin-listener read types hand-written in the console (`gateway/config.rs`) drift from the server's; shared wire types belong in `ferrofed-registry` beside `console`/`operator` (console-ui.md §7; #577).
- Check every new screen and action for its browser journey in `tools/ferrofed-testkit/tests/it/journeys/`; #909 and #577 shipped none (console-ui.md §11).
- A token-refresh slice needs a concurrent test (join_all over one due session, token mock `.expect(1)`); sequential requests prove nothing about the per-session gate (console-ui.md §0, RFC 6749 §6; #643).
- Gateway `without_secrets` (revision/resources.rs) filters by key name only, so a `SecretUrl` value inside a section reaches the console page unredacted; check every rendered settings value against value-based redaction (console-ui.md §0; #578).
- `ConfigForm` is `#[serde(default)]` and the browser reads a missing field as "", so an absent field equals an empty one. Where empty means "remove what is held" (FHIR endpoints, #988), the add-member form or a stale page deletes held data: require a presence marker or `Option` (reliability.md; console-ui.md §5).
- An `<a download>` to a route that may answer a refusal hides it: the browser saves or fails the download and never shows the status and code. `Content-Disposition: attachment` on the 200 is enough (console-ui.md §4; #579).
- A body-limit middleware that reads the request body itself must also bound the read in time: a chunked body sent slowly holds the task before any session check runs; wrap the read in a timeout and answer 408 (RFC 9110 §15.5.9) (#1022).
- A focusable scroll container (`<main tabindex="0">`) still needs a reflow check at 320 CSS px, not 390: WCAG 2.2 SC 1.4.10 names 320 (console-ui.md §10; #1037).
- A layout `<main>` with `overflow-x:auto` makes a document-width reflow check (`Page.getLayoutMetrics`) unable to fail for anything inside it; put the scroll box on each table (WCAG 2.2 SC 1.4.10; console-ui.md §10, §11; #1057).
- A nav `aria-current` tint or a segmented `aria-pressed` that changes only colour fails §10 and WCAG 1.4.1/1.4.11; compute the state-to-state ratio in both themes (console-ui.md §10; #1075).
- An `md:sticky` top bar with no `scroll-padding-top` lets Shift+Tab hide the focused control under it (WCAG 2.2 SC 2.4.11; console-ui.md §10; #1075).
- A form post under `Referrer-Policy: no-referrer` carries no `Referer`, so a handler that redirects keeps the operator's page only through an explicit, checked return path (`prefs::return_path`; console-ui.md §1; #1075).
- A popover nav shown from `md` by an author `display` override works because author rules beat the UA `[popover]:not(:popover-open)` rule; reset `position`, `inset`, `margin`, `border` and `width` too (HTML popover; #1075).
- A second cookie contradicts any "one opaque cookie" text in console-ui.md §0, the session module doc and the data-protection table; update all three with the cookie (#1075).
- A page writes its own markup with `crate::pages::h` from parts escaped with `crate::pages::t`; a value put into `h` raw is an injection, and FerroUI's highlighter escapes with named entities (`&lt;`) where `t` writes numeric ones (`&#60;`), so a hostile-value test names the escape of the part that drew the value (console-ui.md §1; #1457).
- A form that answers on its page must post to the page's path with its query (`pages::here_with_query`): posting to the bare path drew a stored query's newest version in place of the one the address named (console-ui.md §1; #1457).
- `ferrohealth_server::console::headers` writes a policy with a nonce and `'wasm-unsafe-eval'`; the console's `strict_policy` layer must stay outside it so the strict policy wins (console-ui.md §2; #1457).

