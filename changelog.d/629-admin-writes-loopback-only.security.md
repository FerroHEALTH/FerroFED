- The admin listener's write actions answer a loopback peer alone (#629).
  With `metrics.allow_remote = true` a remote peer reads `GET /metrics` and
  nothing else: the stored-query distribution, and any later write or
  administrative action, answers `403 operation-refused` to every peer that
  is not loopback, until the admin listener authenticates its callers
  (#635). `config check` says so for a listener off loopback. In Kubernetes,
  reach the write actions through `kubectl port-forward`. The example's
  network policy stays as defence in depth.
