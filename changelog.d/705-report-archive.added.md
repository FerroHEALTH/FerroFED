- `ferrofed report` writes one tar archive a deployment attaches to a
  complaint or a serious-incident report (Regulation (EU) 2025/327 Art 44):
  the version, the commit and target the release build recorded, the
  bindings, the pins, the release attestations to verify, the effective
  configuration with every credential, URL credential and `[dev]` value
  redacted, and, from the gateway on the same host, readiness, the
  dependency states, the integrity incidents (with `--operator-token-file`)
  and the metrics. A part it cannot read is listed in the manifest with the
  reason. The book's Complaints and incidents page documents the format.
