- The gateway admits `DPoP` requests (RFC 9449) under a new `[auth.dpop]`
  table: the proof is checked against the request and the token's key, and
  a proof sent a second time is refused `401` with the `DPoP` challenge
  `invalid_dpop_proof`. With FerroFED's own store set, every replica records
  each accepted proof in `ferrofed.dpop_proof` by the SHA-256 of its `jti`,
  in the one statement that checks it, so a replay to another replica is
  refused (§11.1), the sweep deletes a proof past its window, and with the
  store down a `DPoP` request is refused `503` (#1637).
