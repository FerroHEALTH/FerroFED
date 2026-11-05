- FerroFED's own PostgreSQL store, `[store]`: one database and login role
  of FerroFED's own, never a CDR's, for the state that must survive a
  restart and be one on every replica. Its connection string is `url` or
  `url_file`, a secret, and must set `sslmode=require` under every profile,
  verified against `ca_file` or the platform's roots; `max_connections`,
  `timeout_ms` and `sweep_interval_s` bound it. Its migrations are part of
  the binary and run at start under an advisory lock, so replicas booting
  together apply each once, and a database a newer release migrated is
  refused. The `store` readiness indicator is down while the database does
  not answer, a request that needs the store is then refused, and a sweep
  deletes the rows past their window. A configuration that declares a
  national contact point issuer or sets `[stored_queries] backend =
  "postgres"` without `[store]` refuses to start, naming both; otherwise the
  gateway runs with no PostgreSQL database (#1636).
