- The operator console runs on the FerroHEALTH shared crates: its
  configuration is read with `ferrohealth-config`, so every job takes
  `--set key=value`, a refusal names the key and the variable or pair that
  set it, and `config check` prints the running configuration with every
  secret redacted; it serves and drains through `ferrohealth-server`, with
  its browser security headers and same-origin check; and it logs through
  `ferrohealth-telemetry`, whose `[telemetry]` section and `RUST_LOG` choose
  what it logs. It serves the family's health routes:
  `GET /health/liveness`, `GET /health/readiness`, which answers `503` once
  the console drains, and `GET /health`, which follows readiness and now
  answers `{"state":"up"}` where it answered `{"status":"up"}`. The example
  Kubernetes Deployment probes liveness and readiness on their own routes.
