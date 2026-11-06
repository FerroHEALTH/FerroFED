- The gateway runs on the family's 0.9 identity line: `ferrohealth-smart`
  0.9.1, `ferrohealth-session` 0.3.1, `ferrohealth-config` 0.2.10, `ferrohealth-ui`
  0.6.8, `ihe-iti` 0.0.76,
  `nl-generic-functions` 0.0.43, and `ihe-atna` and `ehds-logging` 0.4.0
  (#1682). Every token is classified under its issuer's `kind` and profile,
  and one that names no caller the kind admits is refused as an invalid
  token, its `check` `no caller kind` in the security log. A `persons`
  token is admitted only through a client the issuer lists in
  `first_party_clients` or `delegates`, and an RFC 8693 `act` claim only
  from a listed delegate. The access record names how a contact point
  relayed its accessor in `relayed-via` and `relayed-issuer`, and the gate
  refuses a contact point's token whose provider identifier is no
  `urn:oid:` or `urn:hl7ii:` identifier, or whose role is ISCO `44` without
  `on-behalf-of`.
- Every `[[auth.issuer]]` holds its own `audience`, which takes
  `gateway.public_url` when unset; a changed `audience` of an issuer the
  gateway already trusts waits for a restart, named `auth.issuer.audience`
  (#1682).
- An `[[auth.issuer]]` can be set from the environment by element index,
  such as `FERROFED__AUTH__ISSUER__0__KIND=persons`, through
  `ferrohealth-config` 0.2.10 (#1682).
