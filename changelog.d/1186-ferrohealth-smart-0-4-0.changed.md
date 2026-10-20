- `ferrohealth-smart` 0.4.0 (#1186). Every `[<section>.oauth]` table takes
  the keys the crate adds: `client_auth = "tls_client_auth"` or
  `"self_signed_tls_client_auth"` with `client_certificate_file` and
  `client_key_file` for RFC 8705 §2 mutual TLS to the token endpoint,
  `ca_file` for trust roots added to the platform's, and
  `assertion_audience = "issuer"` (RFC 7523 §3). An issuer's metadata and
  key set are still read on its first token, now by the crate; the
  `key-set-unavailable` event is written once per failed read. The answers
  do not change.
