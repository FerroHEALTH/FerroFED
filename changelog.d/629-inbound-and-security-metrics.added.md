- The metrics now show what the gateway's own clients see (#629).
  `ferrofed_http_requests_total` counts every answered request by method,
  route template and status class, and the OpenTelemetry HTTP server
  metrics `http_server_request_duration_seconds` and
  `http_server_active_requests` time them and count those in flight. The
  cross-reference resolver's calls are counted by outcome in
  `ferrofed_resolver_requests_total` and timed, and the localizer and
  demographics calls are timed beside their existing counters.
  `ferrofed_security_events_total` counts every caller refused at client
  authentication by its reason, and every identifier-hygiene event of the
  `ferrofed::security` log, the outbound-gate stops among them, by kind.
  Every label comes from a closed set: a route is a template, never a path,
  and nothing a request sends reaches a label.
- Each release attaches a Grafana dashboard, `ferrofed-dashboard.json`, and
  a Prometheus rule file, `ferrofed-alerts.yaml`, from
  `deploy/observability/`, held to the exported metrics by
  `scripts/checks/observability.sh` and `promtool check rules` in CI. The
  Kubernetes example serves the admin listener on port 9464 of each pod,
  annotates the pods for scraping, and opens that port to the monitoring
  namespace's Prometheus alone in a new `networkpolicy.yaml`.
