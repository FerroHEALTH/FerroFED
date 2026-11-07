- `[credentials."<endpoint id>".onward]`, the onward lane of a node: every
  request a verified caller originates reaches the node under the token the
  table's issuer exchanges the caller's for (RFC 8693 token exchange, or the
  RFC 7523 JWT bearer grant under the Entra profile), through
  `ferrohealth-smart`'s onward client, which checks and caches each issued
  token. A person is carried at a `persons` issuer, a professional of another
  domestic party at a `federation` issuer and a patient at a `patients`
  issuer; every other caller fails that node with nothing sent. A refused
  exchange fails the node and never falls back to the gateway's own token,
  which the lane's `oauth` table keeps for the gateway's own requests
  (#1703).
