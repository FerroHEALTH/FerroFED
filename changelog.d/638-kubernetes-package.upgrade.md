- The Kubernetes example moved to `deploy/kubernetes/base/`: apply it with
  `kubectl apply -k deploy/kubernetes/base` in place of
  `kubectl apply -f deploy/kubernetes/`. Its NetworkPolicy now admits only
  your reverse proxy's namespace (`ingress-nginx` until you edit it) and the
  console to the gateway port, so a client inside the cluster that called
  the gateway directly needs a peer of its own; the StatefulSet's
  `terminationGracePeriodSeconds` is 50 and its governing Service is the
  new `ferrofed-admin`. Kubernetes refuses a change of a StatefulSet's
  `serviceName`, so run `kubectl delete statefulset ferrofed
  --cascade=orphan` before you apply; the pods and their audit spool claims
  stay, and the new StatefulSet adopts them (#638).
