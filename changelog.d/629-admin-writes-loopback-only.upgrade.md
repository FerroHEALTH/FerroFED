- If you run the admin listener's stored-query distribution from another
  host, with `metrics.allow_remote = true`, run it from the gateway's host
  instead, or in Kubernetes through `kubectl port-forward` (#629). It now
  answers `403 operation-refused` to every peer that is not loopback.
  Scraping `GET /metrics` from another host is unchanged.
