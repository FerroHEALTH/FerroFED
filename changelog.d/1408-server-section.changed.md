- A `[server]` value of the wrong type or form (`listen`, `base_path`,
  `trusted_proxies`, a zero `header_read_timeout_ms` or
  `max_concurrent_requests`, a `[server.tls]` without its `certificate_file`
  or `key_file`) is refused as the configuration loads, naming the key,
  where it was refused as the configuration resolved. The address a request
  came from is read by `ferrohealth-server`, behind the same
  `server.trusted_proxies` and `server.forwarded_header`.
