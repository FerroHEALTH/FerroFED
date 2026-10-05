<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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
  navigation bar to the four operator views.
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
- **`GET /health`**, which answers `200` while the process serves, and the
  `healthcheck` command the image runs against it.

Every answer carries a Content-Security-Policy whose script source is a nonce
minted for that answer, `X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY` and `Referrer-Policy: no-referrer`, and no page is
stored by a cache.

## What is planned

- **The query console (#277):** an AQL query, or a stored query by name, run
  through the gateway, with every node's status and latency and whether the
  answer is complete shown plainly, and a refusal's stable code. A patient is
  named through a parameter, and an identifier entered in the console never
  appears in a URL, the browser history or the console's logs.

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
```

With `secure_cookie = true` the session and sign-in cookies carry the
`__Host-` prefix, which binds each to the console's host (RFC 6265bis
§4.1.3.2); with it off, for a loopback trial, they carry none.

Without an `[oidc]` table the console offers no sign-in, and `GET /login`
answers `503`. The provider's URLs must be `https` unless their host is
loopback, and `scopes` must include `openid`. `ferrofed-viewer config check`
reads and checks a configuration without binding a socket.

The image sets `server.listen` to `0.0.0.0:3000` and `server.site_root` to
the site bundle it carries, runs as the numeric user `65532`, and runs
`healthcheck` as its `HEALTHCHECK`.
