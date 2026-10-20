- Client authentication, the SMART on openEHR grants and the gateway's own
  OAuth 2.0 client credentials run on `ferrohealth-smart` 0.3.1, the
  resource-server crate every FerroHEALTH product shares (#1174). Every
  token that does not verify is answered one `401` with
  `error="invalid_token"` and no description (RFC 6750 §3.1, RFC 9068 §4),
  and the security log names the failed check as `check`. Two
  `Authorization` fields, or one that is no credential, are a `400`
  `credential-malformed` with `error="invalid_request"`. A `403` names the
  error code and no scope. An issuer's metadata and key set are read when
  its first token arrives, so an issuer that cannot be reached is a `503`
  `authentication-unavailable` then, never a refused start.
- The metrics scrape and the PMIR feed authenticate HTTP Basic users of
  `[[auth.basic.users]]`, named by `metrics.scrape_users` and
  `pmir.feed_users`; one from another Basic user is answered `403`.
