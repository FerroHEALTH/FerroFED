---
name: review-hazards
description: Leptos, OIDC and operator-surface hazards confirmed in FerroFED console reviews (PR #580 onward), one line each with the rule citation
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
- Bundle growth: until #585 lands a budget and a CI measurement in leptos-ui.md, ask for raw and compressed bytes in the PR body; after it, check the PR against that budget (leptos-ui.md §1).
- Watch for a PR that edits `.claude/rules/leptos-ui.md` or `/ui-gates` to drop its own test obligation (journeys). Return that to the orchestrator (leptos-ui.md §11, "Never weaken a gate").
