- `[server]` is the section of `ferrohealth-server`, and the gateway's own
  keys moved to a new `[gateway]` table: move `server.public_url`,
  `server.replica` and `server.bindings_drain_timeout_ms` there, and rename
  `server.overload_retry_after_s` to `server.retry_after_s`. This release
  still reads each old key under its new name and names it in `config check`
  and the start-up log; FerroFED 0.0.16 refuses it, and setting an old key
  beside its new one is refused now. The `FERROFED__` overrides follow:
  `FERROFED__GATEWAY__PUBLIC_URL`, `FERROFED__GATEWAY__REPLICA`.
- `server.base_path` writes the root as the empty path, its default: remove
  `base_path = "/"`, or write `base_path = ""`. A `/` is refused.
- `[server.caller_rate]` needs both `requests_per_second` and `burst`, where
  each defaulted (10 and 20) before.
