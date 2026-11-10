- Write the spans of time of `ferrohealth-smart` 0.15 as durations under
  keys that name no unit: on each `[[auth.issuer]]` `clock_skew`,
  `jwks_max_age`, `jwks_refresh_interval` and `timeout`;
  `introspection.cache_max_age`; `[auth.dpop]` `max_age` and `clock_skew`;
  on each `[credentials."<id>".credentials.oauth]` `refresh_margin`, `timeout` and
  `previous_key_overlap`; and `[onward.keys] max_onward_lifetime`, until
  now a bare number of seconds. The old keys are refused as unknown keys
  (#1799).
- Write `[auth] scopeless_subjects` entries as tables with `issuer` and
  `sub` (#1799).
- Move `[trust.issuer.patient] act_is_representative` to
  `act_represents_patient = true` on the `[[auth.issuer]]` of `kind =
  "patients"`. The old key refuses the start, naming its replacement. A
  patient token that carries an RFC 8693 `act` claim on an issuer without
  it is now refused as an invalid token (#1799).
- In the operator console, write `[oidc] timeout` and `[session]`
  `sign_in_timeout`, `idle_timeout` and `absolute_timeout` as durations,
  and move `[oidc] ca_file` to `[oidc.tls] trust_roots_file` (#1799).
