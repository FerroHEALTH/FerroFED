- A rolling restart no longer cuts requests in flight (#625). The drain,
  `server.shutdown_timeout_ms`, now defaults to `server.request_timeout_ms`,
  and `config check`, `serve` and a reload refuse a drain shorter than the
  request timeout, naming both keys. The new `server.drain_delay_ms` keeps
  the listener accepting for that long after `SIGTERM`, with readiness
  already `503`, so a load balancer stops routing to the gateway before it
  stops taking requests; it is `0` by default. The example configurations
  drain for 30 seconds, the Kubernetes example waits 5 seconds first, and
  the grace periods outlast both: `terminationGracePeriodSeconds: 45` and
  `stop_grace_period: 40s`. A configuration that set a drain shorter than
  its request timeout, such as the earlier examples' 10 seconds beside 30,
  is refused until the drain is raised or removed.
