<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The operator console (planned screens)

The operator console is a web interface to a FerroFED federation, for the
people who run it. It ships as its own binary, `ferrofed-viewer`, and its own
image, `ghcr.io/ferrohealth/ferrofed-viewer`, beside the gateway. No
specification governs the console: it is FerroFED's own design.

The console is one more client of the gateway. It reaches the gateway over
HTTP, on the same public surface any other client uses, and it adds no
behaviour to the gateway. It holds no clinical data.

## What is built

The console's skeleton is built; its screens are not.

- **A landing page** that names the console and offers sign-in.
- **Operator sign-in at an OpenID Provider:** `GET /login` redirects the
  browser to the provider's authorization endpoint with the authorization
  code grant and a PKCE challenge (RFC 6749 §4.1, RFC 7636). The sign-in
  `state` and the PKCE verifier stay on the console's server, in a session
  the browser knows only by an opaque `HttpOnly` cookie. The provider's
  redirect back to `/auth/callback` is checked against that session. The
  code exchange is planned with the operator views (#276); until it lands,
  the callback answers `501`. Sessions live in the console's memory: a
  restart ends every session, and more than one replica needs a load
  balancer that keeps an operator on one replica.
- **The gateway client** the screens will use: the gateway's self-description,
  `OPTIONS {base}/`, read into its typed form, and the ITS-REST surface under
  `{base}/v1`, each called with the signed-in operator's own access token.
- **`GET /health`**, which answers `200` while the process serves, and the
  `healthcheck` command the image runs against it.

Every answer carries a Content-Security-Policy whose script source is a nonce
minted for that answer, `X-Content-Type-Options: nosniff`,
`X-Frame-Options: DENY` and `Referrer-Policy: no-referrer`, and no page is
stored by a cache.

## What is planned

- **The operator views (#276):** the registry members and their membership
  standing, node health and latency, integrity incidents and the learned
  `creating_system_id` map, the stored queries, and the gateway's
  self-description. No view shows a patient identifier or clinical data.
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
file through `client_secret_file`.

```toml
[server]
listen = "0.0.0.0:3000"
site_root = "/site"

[gateway]
base_url = "https://gateway.example.org/"
timeout_ms = 30000

[session]
secure_cookie = true
idle_timeout_s = 1800
max_sessions = 10000

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/auth"
client_id = "ferrofed-viewer"
client_secret_file = "/run/secrets/viewer-client-secret"
redirect_uri = "https://console.example.org/auth/callback"
scopes = ["openid"]
```

Without an `[oidc]` table the console offers no sign-in, and `GET /login`
answers `503`. The provider's URLs must be `https` unless their host is
loopback, and `scopes` must include `openid`. `ferrofed-viewer config check`
reads and checks a configuration without binding a socket.

The image sets `server.listen` to `0.0.0.0:3000` and `server.site_root` to
the site bundle it carries, runs as the numeric user `65532`, and runs
`healthcheck` as its `HEALTHCHECK`.
