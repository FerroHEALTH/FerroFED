- `deploy/kubernetes/` is a kustomize package, and every release attaches
  it as `ferrofed-vX.Y.Z-kubernetes.tar.gz` with the gateway and console
  images pinned by digest. Its base adds a `preStop` sleep matching the
  drain delay, topology spread over zones and nodes, a headless Service for
  the admin listeners and a NetworkPolicy that admits the reverse proxy,
  the scraper and the console alone; its components add the operator
  console, an Ingress or a Gateway API route for each, a Prometheus
  Operator ServiceMonitor and an egress policy. The book's Kubernetes page
  says how a changed configuration is reloaded without a restart (#638).
