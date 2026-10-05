<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console

The operator console is a web interface to a FerroFED federation, for the
people who run it. It ships as its own binary, `ferrofed-viewer`, and its own
image, `ghcr.io/ferrohealth/ferrofed-viewer`, beside the gateway. No
specification governs the console: it is FerroFED's own design.

The console is one more client of the gateway. It reaches the gateway over
HTTP, on the same public surface any other client uses, and it adds no
behaviour to the gateway. It holds no clinical data.

## What is built

- **A landing page** that names the console and offers sign-in, and a
  navigation bar to the four operator views and the query console.
- **Operator sign-in at an OpenID Provider:** `GET /login` redirects the
  browser to the provider's authorization endpoint with the authorization
  code grant, a `nonce` and a PKCE challenge (RFC 6749 §4.1, RFC 7636). The
  `state`, the `nonce` and the PKCE verifier stay on the console's server as
  a pending sign-in, which the browser knows only by an opaque `HttpOnly`
  cookie that expires with it. The provider's redirect back to
  `/auth/callback` is checked against it once. The code then goes to the
  provider's token endpoint with the PKCE verifier (RFC 6749 §4.1.3, RFC 7636
  §4.5), with the client secret as HTTP Basic for a confidential client. The
  ID Token that comes back must verify against the provider's JWK Set and
  carry the configured issuer, the console's client id as its one audience,
  an `exp` still ahead, and the sign-in's `nonce` (OpenID Connect Core 1.0
  §3.1.3.7). An ID Token for more than one audience, or signed with a shared
  secret, is refused. Only then does a signed-in session begin, holding the
  operator's access token on the server, and the browser goes back to `/`. A
  session the browser already held ends when the new one begins. An ID Token that fails a check is
  `401`, and a provider that refuses or cannot be reached is `502`.
- **Operator sign-out:** the navigation bar's "Sign out" button posts to
  `POST /logout`, which ends the signed-in session on the console's server
  and removes the session cookie. Where `[oidc]` names the provider's
  `end_session_endpoint`, the console then redirects the browser there with
  the session's ID Token as `id_token_hint`, its `client_id`, and the
  `post_logout_redirect_uri` you registered with the provider (OpenID
  Connect RP-Initiated Logout 1.0 §2); without one the browser goes back to
  `/`. The console takes a sign-out only from its own pages: a request the
  browser marks `Sec-Fetch-Site: same-origin`, or, without fetch metadata,
  one whose `Origin` is the origin of `redirect_uri`. Any other is `403`
  and leaves the session as it was, and a `GET /logout` is `405`.
- **Two separate pools of server-side state.** Pending sign-ins live for
  `sign_in_timeout_s` and are bounded by `max_sign_ins`; a full pool drops
  its oldest pending sign-in, so a flood of `GET /login` holds at most that
  many and stops costing anything once it ends. A signed-in session is
  created only once a sign-in completes, lives for `idle_timeout_s` without
  a request and `absolute_timeout_s` at most, and is bounded by
  `max_sessions`, which refuses a new session rather than evicting one. No
  sign-in traffic ever removes a signed-in session. The console does not
  limit sign-ins per client: behind a load balancer it cannot tell clients
  apart without trusting a forwarded address, so put a rate limit for
  `/login` at the edge. Both pools live in the console's memory: a restart
  ends every session, and more than one replica needs a load balancer that
  keeps an operator on one replica.
- **The operator views**, each rendered on the console's server from the
  gateway's own surface, called with the signed-in operator's own access
  token:

  | View | What it shows | Read from |
  |---|---|---|
  | `/members` | every member endpoint, its organisation, membership standing, last observed health, node, `system_id`, product and median latency, and the state of every other dependency | `OPTIONS {base}/` and `GET {base}/health/dependencies` |
  | `/integrity` | the integrity incidents of each kind since the gateway started, the most recent ones, and the `creating_system_id` routing table, 100 rows a page | `GET {base}/operator/incidents` and `/operator/creating-systems` |
  | `/stored-queries` | every stored-query version the gateway holds, with its AQL, 100 versions a page | `GET {base}/operator/stored-queries` |
  | `/federation` | the gateway's self-description | `OPTIONS {base}/` |

  A paged view names the rows it shows and how many there are, and links
  the page before and after it by `?offset=`. A view without a signed-in
  session sends the browser to `/login` before the gateway is asked
  anything, whatever the case of its path or a trailing slash, and each
  server function a view loads through checks the session itself. A view
  the gateway refuses shows the gateway's status and its stable error code;
  a `401` links back to sign-in. A view whose answer the console cannot read
  says so with the status, and one the gateway cannot answer says so too; no
  view is ever silently empty. The `/operator/` routes need a token
  carrying the [operator scope](authentication.md#the-operator-surface) its
  issuer names. No view shows a patient identifier, a token or clinical
  data.
- **The query console, `/query`:** an AQL query, or a stored query by name
  and optional version, runs through the gateway's own query surface,
  `POST {base}/v1/query/aql` or `POST {base}/v1/query/{name}`, as the
  signed-in operator, exactly as any client sends it. The form offers what
  the gateway's `OPTIONS {base}/` declares: the member endpoints and
  organisations to target (`openEHR-federation-endpoint`,
  `openEHR-federation-organisation`, §8.4), its dedup modes
  (`openEHR-federation-dedup`, §10), and the best-effort opt-in
  (`openEHR-federation-completeness: partial`, §11.4) where it offers one.
  `offset` and `fetch` are optional. The answer shows the rows under
  `columns[]` as the gateway renders them, and every endpoint of
  `meta.federation.endpoints[]` with its status, latency, row count and
  error. An answer whose `meta.federation.complete` is `false` says
  "Incomplete answer" in words before its rows. A `504` or `424` the gateway
  answers under its all-or-nothing default shows its status and the
  endpoints that failed, from the diagnostic envelope it carries (§11.4). A
  refusal shows its status and stable error code, and a result set with no
  readable `meta.federation` is shown as unreadable, never as an answer.

  Name a patient through a parameter, one `name=value` per line, never in
  the AQL text; each value is sent as a string. The form posts its fields in
  the request body to the console, which sends them to the gateway in the
  request body, so nothing you enter reaches a URL or the browser history.
  The fields carry `autocomplete="off"`, and the console logs a refusal by
  its status and code alone, never the query or a parameter.
- **`GET /health`**, which answers `200` while the process serves, and the
  `healthcheck` command the image runs against it.

Every answer carries a Content-Security-Policy whose script source is a nonce
minted for that answer, `X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY` and `Referrer-Policy: no-referrer`, and no page is
stored by a cache.

The query console renders its answer in the browser, so it needs the
console's WebAssembly bundle: without it the form still posts, and the
browser shows the answer as the console's JSON.

## Why a console of its own

The gateway authenticates every request by bearer token and holds no browser
session ([Client authentication](authentication.md)). The console holds the
operator's session on its own server and calls the gateway with the
operator's token, so the gateway's authentication stays as it is. The gateway
also serves no static files: it answers `501` on a path it does not serve,
and its request log carries no body and no header value.

## Configuration

The console reads a TOML file named by `--config` or `FERROFED_VIEWER_CONFIG`.
Every key can be set from the environment as
`FERROFED_VIEWER__<SECTION>__<KEY>`, and the client secret can be read from a
file through `client_secret_file`. A refused configuration names the key and
the line, never a value, so no secret reaches a message or a log.

```toml
[server]
listen = "0.0.0.0:3000"
site_root = "/site"

[gateway]
base_url = "https://gateway.example.org/"
timeout_ms = 30000

[session]
secure_cookie = true
sign_in_timeout_s = 300
max_sign_ins = 1000
idle_timeout_s = 1800
absolute_timeout_s = 43200
max_sessions = 10000

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth"
token_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/token"
jwks_uri = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs"
client_id = "ferrofed-viewer"
client_secret_file = "/run/secrets/viewer-client-secret"
redirect_uri = "https://console.example.org/auth/callback"
scopes = ["openid"]
end_session_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/logout"
post_logout_redirect_uri = "https://console.example.org/"
```

With `secure_cookie = true` the session and sign-in cookies carry the
`__Host-` prefix, which binds each to the console's host (RFC 6265bis
§4.1.3.2); with it off, for a loopback trial, they carry none.

Without an `[oidc]` table the console offers no sign-in, and `GET /login`
answers `503`. The provider's URLs must be `https` unless their host is
loopback, and `scopes` must include `openid`. `end_session_endpoint` is
optional, and `post_logout_redirect_uri` needs it. `ferrofed-viewer config check`
reads and checks a configuration without binding a socket.

The image sets `server.listen` to `0.0.0.0:3000` and `server.site_root` to
the site bundle it carries, runs as the numeric user `65532`, and runs
`healthcheck` as its `HEALTHCHECK`.
