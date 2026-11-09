- The gateway runs on `ferrohealth-smart` 0.13.0, `ferrohealth-session`
  0.7.0, `ihe-atna` and `ehds-logging` 0.8.0, `ihe-iti` 0.0.83 and
  `nl-generic-functions` 0.0.50 (#1748, #1766):
  - An `[[auth.issuer]]` URL that carries a user name or password, and a
    `jwks_file` that holds a symmetric key or private key material, refuse
    the start.
  - The published exchange profile of a national contact point's
    authorization server
    ([National contact points](https://ferrofed.eu/docs/integrate/national-contact-point.html))
    names two more obligations: it authenticates the national connector
    before it reads an assertion, since a `sender-vouches` assertion names
    no presenter (eHDSI SAML Profile 10.0.0 §2.1), and it admits each
    exchange once, in one replay record every replica shares. The harness
    authorization server the test suite runs does both.
  - A refusal of a JSON document the gateway could not read, such as a
    key set that is no JWK Set or a malformed token endpoint answer, names
    the reader's category, line and column, and no longer any part of the
    document (FerroHEALTH/FerroAUTH#351).
