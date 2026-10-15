- A routed read or write a member answers with a `4xx`, such as a `404` for
  a composition it does not hold or a `412` for a stale `If-Match`, now
  counts as `ferrofed_node_requests_total{outcome="node-error"}`. The
  `FerroFEDMemberFailing` rule of `ferrofed-alerts.yaml` reads that series,
  so a client that sends many such requests to one member can now raise it;
  check the rule's threshold against your clients' traffic before you
  upgrade (#982).
