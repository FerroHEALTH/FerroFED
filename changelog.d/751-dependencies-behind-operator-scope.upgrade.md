- Read the dependency report at `GET {base}/operator/dependencies` with a
  token that carries your issuer's `operator_scope` (#751). `GET
  {base}/health/dependencies` is no longer served and answers `404`, so move
  every monitor that read it, and give it an operator token. `ferrofed report`
  reads the report with `--operator-token-file`. A proxy rule that kept
  `/health/dependencies` inside can go; the health family left open,
  `{base}/health` and `{base}/health/readiness`, names no member.
