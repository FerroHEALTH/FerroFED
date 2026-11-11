- The compose quickstart and the end-to-end harness run FerroEHR 5.7.0, node
  and database image alike, pinned by digest, in place of 5.6.3, and the
  gateway reaches every quickstart node under the caller. Each node trusts
  the quickstart's development issuer as its issuer of persons, binds every
  person who signs in to a `PERSON` it creates at the first request, and
  admits the gateway only as a delegate acting for the person a token names:
  each node's endpoint has an onward lane, so a caller's query reaches the
  node under the token the issuer exchanges the caller's token for (RFC
  8693), and no node is reached under a Basic user. The nodes run in the
  issuer's network namespace, each on its own port, and four
  `ferroehr-definitions-*` one-shots load FerroEHR's professional `PERSON`
  and `ROLE` templates, vendored under `docs/specs/ferroehr-definitions/`,
  before the nodes start. A quickstart volume from an earlier release holds
  no such templates or persons: run `docker compose down -v` once before you
  bring the stack up again (#1615).
- The release compose example reaches node B through an onward lane in place
  of a user and a password, and the onward credentials page states what a
  FerroEHR 5.7.0 node lists for the gateway: the onward issuer as a `persons`
  issuer with the node's own audience, and the gateway's client among its
  delegates. From this release a deployment whose FerroFED gateway writes to
  FerroEHR adds the node's `persons` issuers once every FerroEHR endpoint has
  its onward lane (#1615).
- The gateway runs on `ferrohealth-smart` 0.19.1, which signs each client
  assertion of a grant for 60 seconds in place of 300, the lifetime the
  quickstart's development issuer admits on a token exchange (#1615).
