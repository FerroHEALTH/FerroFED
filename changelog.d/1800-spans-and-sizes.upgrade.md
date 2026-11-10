- Write every span of time and amount of data of FerroFED's own sections
  as a duration (`"30s"`, `"5min"`) or a size (`"16MiB"`) under a key that
  names no unit: `federation.per_node_timeout`, `overall_timeout`,
  `binding_ttl` and `max_node_answer_size`, `federation.localization.timeout`,
  `gateway.bindings_drain_timeout`, `signing.node_jwks_cache` and
  `rotation_overlap`, `emergency.window`, `config.poll` and `confirm`,
  `store.timeout` and `sweep_interval`, `fhir.document_url_lifetime`,
  `eehrxf.terminology.timeout`, `pdqm.timeout`, `pmir.timeout` and
  `check_interval`, `nl_gf.mitz.timeout`, `registry.mcsd.refresh_interval`,
  `deadline` and `max_size`, `audit.repository.spool_max_size`,
  `spool_write_timeout`, `timeout` and `retry_max`, and
  `xcpd.audit_repository.spool_max_size`, `spool_write_timeout`,
  `connect_timeout`, `send_timeout` and `retry_max`, with the
  `FERROFED__` overrides to match. Each old key (`per_node_timeout_ms`,
  `window_s`, `spool_max_bytes` and the others) refuses the start and
  `config check`, naming its replacement. The operator console writes
  `gateway.timeout` in place of `gateway.timeout_ms` (#1800).
