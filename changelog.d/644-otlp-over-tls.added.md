- `telemetry.otlp_endpoint` and `metrics.otlp_endpoint` take an `https://`
  collector, reached over TLS with the platform's trust roots, and
  `[telemetry.otlp_tls]` and `[metrics.otlp_tls]` add `trust_roots_file`
  and a client identity (`client_identity` or `client_identity_file`) for
  mutual TLS, each read and checked at start, so a remote collector needs
  no forwarder beside the gateway (#644).
