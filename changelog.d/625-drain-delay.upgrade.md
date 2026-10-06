- Set `server.shutdown_timeout_ms` to at least `server.request_timeout_ms`,
  or remove it so it takes the request timeout (#625). `config check` and
  `serve` now refuse a shorter drain, and the v0.0.9 example configuration
  set 10000 beside a request timeout of 30000. Then give the container
  longer to stop than `server.drain_delay_ms` plus the drain: the examples
  use `stop_grace_period: 40s` in Compose and
  `terminationGracePeriodSeconds: 45` in Kubernetes.
