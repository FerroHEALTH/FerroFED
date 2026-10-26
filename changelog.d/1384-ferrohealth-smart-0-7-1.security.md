- `ferrohealth-smart` 0.7.1 masks the userinfo of an OAuth section's
  `issuer` and `token_endpoint` in every rendering, a URL that does not parse
  included; before it a credential written into such a URL could reach a
  log through the section's `Debug` or serialized form (CWE-532,
  GHSA-vphj-3f8c-5hpx). FerroFED takes it with the releases built on it:
  `ihe-atna` and `ehds-logging` 0.2.11, `ihe-iti` 0.0.60,
  `nl-generic-functions` 0.0.30 and `ferrobridge-term` 0.1.137.
