- Move the address, timeouts, TLS and credential of `[audit.repository]`
  and `[eehrxf.terminology]` into their new `endpoint` table,
  `ferrohealth-smart`'s outbound table. `[audit.repository]` keeps
  `hostname`, `source_id`, `enterprise_site` and the `spool_*` keys, and
  `[audit.repository.endpoint]` takes `url`, `timeout` (from `timeout_ms`),
  `retry.max_backoff` (from `retry_max_ms`, the wait between two delivery
  rounds) and `tls`. `[eehrxf.terminology.endpoint]` takes `url`, `timeout`
  (from `timeout_ms`), `tls` and `credentials`; one call is tried once
  unless `retry.max_attempts` says more. Each old key refuses the start and
  `config check`, naming its replacement (#1805).
