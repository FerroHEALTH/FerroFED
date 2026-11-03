- Write `server.public_url`, `server.replica` and
  `server.bindings_drain_timeout_ms` as `gateway.public_url`,
  `gateway.replica` and `gateway.bindings_drain_timeout_ms`, and
  `server.overload_retry_after_s` as `server.retry_after_s`, with the
  `FERROFED__` overrides to match (`FERROFED__GATEWAY__PUBLIC_URL`). This
  release refuses each old key at start-up and in `config check` (#1490).
