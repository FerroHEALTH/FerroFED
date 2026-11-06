- The flat `[auth] issuer = "…"` form and the `[auth]` keys `audience`,
  `algorithms`, `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds`, `jwks_file`, `timeout_ms`, `mode` and
  `role_claims`, each refused naming the `[[auth.issuer]]` table it moved
  to (#1682).
- The `[[trust.issuer]]` keys `introspection_endpoint`, `client_id`,
  `client_secret` and `client_secret_file`: an introspected issuer is an
  `[[auth.issuer]]` with `mode = "introspection"` (#1682).
