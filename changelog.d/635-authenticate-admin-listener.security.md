- The admin listener authenticates its callers (#635). A write action, such
  as the stored-query distribution, needs an access token that client
  authentication verifies and that carries its issuer's `operator_scope`,
  from any peer, loopback included; it answers `401 unauthenticated` or
  `403 scope-insufficient` otherwise. Only `profile = "development"` still
  admits a loopback peer without a credential. `GET /metrics` takes an
  optional bearer token, `[metrics] scrape_token_file`, compared in
  constant time, and outside the development profile a listener off
  loopback is refused unless that token or `[metrics.tls] client_ca_file`
  authenticates the scrape. Each refusal is counted as `admin-write-refused`
  or `scrape-refused`. The Kubernetes example sets a scrape token, and the
  diagnostic report reads `GET /metrics` with it.
