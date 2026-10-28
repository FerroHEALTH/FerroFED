- The health routes are the FerroHEALTH family's, served by
  `ferrohealth-server` 0.3.7 over the indicator registry and lifecycle of
  `ferrohealth-health` 0.1.1; FerroFED's own registry, lifecycle and
  readiness document are deleted (#1397). `GET {base}/health/liveness` is
  the liveness route, `200` while the process serves. `GET {base}/health`
  now answers the status of readiness with `{"state":"up"}` or
  `{"state":"down"}`, so it is `503` while the gateway boots or drains.
  `GET {base}/health/readiness` lists `indicators` as an array of
  `{"name", "state"}` sorted by name, where it was an object keyed by name,
  and carries no `detail`. Point a liveness probe at
  `{base}/health/liveness`: the example Kubernetes StatefulSet does, and a
  deployment that copied it before moves its `livenessProbe` path from
  `/health`, or the kubelet restarts a gateway that is draining. A monitor
  that parsed the readiness body reads the array.
