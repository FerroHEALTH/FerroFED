- An issuer written with userinfo (`https://user:password@…`) in
  `[[auth.attributes]]`, `[[auth.federation]]`, `[[auth.party]]` or
  `auth.scopeless_subjects` no longer shows its password in
  `ferrofed config check` or in the configuration's `Debug` rendering:
  it renders as `https://***@…`, as `[[auth.issuer]]` already did. The
  gateway moves to `ferrohealth-smart` 0.19, which holds the issuer of
  every configuration table as a `SecretUrl`, with `ferrohealth-session`
  0.14, `ihe-atna` and `ehds-logging` 0.13, `ferrobridge-term` and
  `ferrobridge-validator` 0.6, `eehrxf` 0.7, and `ihe-iti` and
  `nl-generic-functions` 0.5, the releases on the same line (#1864).
