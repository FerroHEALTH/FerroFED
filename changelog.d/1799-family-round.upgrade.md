- Move the `[server]` keys of `ferrohealth-server` 0.8 to their durations
  and sizes: `request_timeout`, `header_read_timeout`, `idle_timeout`,
  `drain_delay`, `shutdown_timeout`, `retry_after` (whole seconds),
  `body_limit`, `compression_min_size` and `[server.tls]`
  `reload_interval`, as in `request_timeout = "30s"`. The old keys
  (`request_timeout_ms` and the others) are refused as unknown keys
  (#1799).
- Move `[telemetry.otlp]` `client_identity_file` and `trust_roots_file`
  into `[telemetry.otlp.tls]`, which also takes `min_version` (#1799).
