- Remove any user name or password from an `[[auth.issuer]]` URL, and keep
  only public keys in an issuer's `jwks_file`: `ferrohealth-smart` 0.13.0
  refuses the start on either (#1748).
- A national contact point's authorization server that answers the exchange
  profile authenticates the national connector before it reads an
  assertion, keeps one record of admitted exchanges that every replica
  shares, and refuses a replay `invalid_grant`
  ([National contact points](https://ferrofed.eu/docs/integrate/national-contact-point.html), #1748).
