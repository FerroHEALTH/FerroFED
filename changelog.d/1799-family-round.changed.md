- Moved `ferrohealth-config` to 0.4.1, `ferrohealth-telemetry` to 0.4.0,
  `ferrohealth-server` to 0.8.0, `ferrohealth-usage-report` to 0.4.0 and
  the performance gate's `ferrohealth-perf` to 0.8.0. The OTLP collector's
  TLS is read and handed to the export by `ferrohealth-server`'s
  `telemetry::build`, and `[metrics.tls]` is the family's listener TLS
  table, so the gateway keeps no copy of either (#1799).
