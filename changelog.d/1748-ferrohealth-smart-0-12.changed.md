- The gateway runs on `ferrohealth-smart` 0.12.0, `ferrohealth-session`
  0.6.0, `ihe-atna` and `ehds-logging` 0.7.1, `ihe-iti` 0.0.82 and
  `nl-generic-functions` 0.0.49 (#1748, #1766):
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
