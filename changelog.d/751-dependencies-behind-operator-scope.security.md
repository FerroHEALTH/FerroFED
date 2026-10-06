- The dependency report, which names every member endpoint and which are
  down, moved to the read-only operator surface, `GET
  {base}/operator/dependencies`, behind the issuer's `operator_scope`, so a
  reader without a token no longer learns the federation's membership (#751).
  The operator console and `ferrofed report` read it there with the
  operator's token. It is no probe, so the overload limit now holds it.
