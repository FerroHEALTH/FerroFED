- A key v0.0.16 renamed (`server.public_url`, `server.replica`,
  `server.bindings_drain_timeout_ms` and `server.overload_retry_after_s`) is
  now refused at start-up and in `config check` naming the key that replaces
  it, whether it is set in the file or as a `FERROFED__` override (#1653).
