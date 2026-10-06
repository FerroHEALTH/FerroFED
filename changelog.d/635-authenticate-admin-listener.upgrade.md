- If `[metrics] listen` names an address other than a loopback one outside
  `profile = "development"`, set `[metrics] scrape_token_file` to a file
  holding a scrape token and give your Prometheus the same token with
  `authorization.credentials_file`, or set `[metrics.tls] client_ca_file`
  (#635). Without either, `serve` and `config check` refuse the
  configuration. The Kubernetes example reads the token from the
  `metrics-scrape-token` key of the `ferrofed-secrets` Secret.
- To run the admin listener's stored-query distribution outside the
  development profile, set `operator_scope` on the `[[auth.issuer]]` your
  operators use and send a token carrying it (#635). A loopback peer
  without a token is now refused `401`, where it was served before.
