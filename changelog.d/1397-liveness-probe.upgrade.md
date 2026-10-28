- Move a liveness probe of the gateway from `GET {base}/health` to
  `GET {base}/health/liveness`. `{base}/health` now follows readiness and
  answers `503` while the gateway boots or drains, so a liveness probe left
  on it restarts a gateway that is draining (#1397).
