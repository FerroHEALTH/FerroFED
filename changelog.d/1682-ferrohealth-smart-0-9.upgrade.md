- Write each issuer as an `[[auth.issuer]]` table with its `kind`:
  `persons` for your professionals, `patients` for an issuer whose
  `patient/` grants `[trust.issuer.patient]` binds, `machines` for systems
  under the client credentials grant alone, or `federation` with its
  `[[auth.federation]]` table. Move `audience`, `algorithms`,
  `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds`, `jwks_file`, `timeout_ms` and
  `role_claims` from `[auth]` onto every issuer table they held for, and
  name the issuer's `profile` when it is not a standards-only server
  (`entra`, `keycloak`, `okta` or `nl`). The start refuses a section-level
  key and an issuer without `kind`, naming each (#1682).
- On each `persons` issuer, list your own applications in
  `first_party_clients`, and a third-party client or an RFC 8693 actor in
  `delegates`; a person's token through any other client is refused
  (#1682).
- Move an introspected issuer from `[[trust.issuer]]` to its own
  `[[auth.issuer]]` with `mode = "introspection"` and an
  `[auth.issuer.introspection]` table: `endpoint`, and the client
  authentication in `[auth.issuer.introspection.client]` (`client_id`,
  `client_secret` or `client_secret_file`). Keep its `[[trust.issuer]]`
  policy, without the introspection keys (#1682).
