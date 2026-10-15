- A reload names a changed `server.public_url`, `server.trusted_proxies`,
  `server.forwarded_header`, `server.replica` or `signing.rotation_overlap_s`
  under the settings that take effect only on a restart; it kept their start
  values without saying so. A test changes every key of a maximal
  configuration in turn and holds each to being applied by a reload or named
  as needing a restart (#1017).
