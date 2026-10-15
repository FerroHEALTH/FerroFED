- The operator console refreshes the operator's access token on its server
  when the provider issued a refresh token (RFC 6749 §6), within 60 seconds
  of its expiry or in the second half of a shorter lifetime, so a session
  lasts to its idle and absolute timeouts. A
  rotated refresh token replaces the old one, the refresh token never
  reaches the browser or a log line, and a refused or failed refresh ends
  the session and sends the operator to sign in again. The new
  `[oidc] offline_access` setting, off by default, asks for the
  `offline_access` scope with `prompt=consent` (OpenID Connect Core 1.0
  §11); `oidc.scopes` no longer admits `offline_access` itself (#643).
