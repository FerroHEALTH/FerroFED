- `OPTIONS {base}/` reports `latency_ms_p50` and `latency_ms_p95` for each
  member endpoint, the median and the 95th percentile of the last 100
  requests the gateway sent it, and leaves both out for an endpoint it has
  not asked; the operator console shows the two side by side on the node,
  endpoint and self-description lists and in the topology (#1220).
