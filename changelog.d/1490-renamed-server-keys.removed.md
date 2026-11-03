- The four `[server]` keys that 0.0.15 still read under their old names:
  `server.public_url`, `server.replica` and
  `server.bindings_drain_timeout_ms`, now only `gateway.public_url`,
  `gateway.replica` and `gateway.bindings_drain_timeout_ms`, and
  `server.overload_retry_after_s`, now only `server.retry_after_s`. An old
  key is an unknown key, which `serve` and `config check` refuse by its
  name, and `config check` and the start-up log no longer name deprecated
  keys (#1490).
