- `[auth]` is now the `ferrohealth-smart` section every FerroHEALTH product
  reads (#1174). Move each issuer's FerroFED policy (`backend_clients`,
  `demographic_clients`, `client_tokens_act_for_professional`,
  `professional_issuing_authority`, `[auth.issuer.patient]`,
  `[auth.issuer.requester]`, `[auth.issuer.assurance]`,
  `[auth.issuer.national_contact_point]` and an introspected issuer's
  `introspection_endpoint`, `client_id` and `client_secret_file`) to a
  `[[trust.issuer]]` entry naming the same `issuer`, and `auth.mode`,
  `[auth.edge]` and `[auth.purpose_of_use]` to `trust.mode`, `[trust.edge]`
  and `[trust.purpose_of_use]`. Drop `jwks_uri` from each issuer: the key
  set is read from the issuer's RFC 8414 metadata, or from `jwks_file`.
  Replace each issuer's `operator_scope` with the one `auth.product_scope`,
  a string starting with `__` or a full URI you control. `auth.audience` is
  a list, and the clock skew, key set and timeout keys are
  `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds` and `timeout_ms`.
- Every static bearer token is gone (#1163). Replace `bearer_token` and
  `bearer_token_file` in a `[credentials."<endpoint>"]` section or a
  service's credentials with `user` and `password_file`, or with an `oauth`
  table, the client credentials grant of `ferrohealth-smart`; Mitz, the NVI
  and the terminology server take `user` and `password_file`. Replace
  `[metrics] scrape_token_file` with `scrape_users`, naming users of
  `[[auth.basic.users]]` whose Argon2id `password_hash_file` you write, and
  give Prometheus `basic_auth`. Replace `[pmir] feed_token_file` with
  `feed_users`, naming the Registry's own `[[auth.basic.users]]` user, and
  give the Registry its password.
- An `oauth2` section that asks for a plain client-credentials token, with
  no token exchange, no `DPoP` key, no binding to or authentication by the
  TLS client certificate and an assertion naming the token endpoint, is
  refused: move it to an `oauth` table, whose `private_key_jwt` signs with
  its own `private_key_file`.
