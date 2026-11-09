- A JSON document the gateway could not read no longer reaches its log
  (GHSA-r5p6-r9m2-c3pw, CWE-532, CWE-209; upstream GHSA-mhv7-3975-6xq3).
  Before it, `ferrohealth-smart` before 0.13.0 refused such a document
  with a source error quoting the value the reader stopped at, and the
  gateway printed that chain when a start failed and logged it at `warn`
  when a node's grant obtained no token. A `jwks_file` holding a private
  or symmetric key, or a token endpoint answer of another shape, could put
  key material or an access token into standard error or the log. This
  release runs on `ferrohealth-smart` 0.13.0, whose refusals name only the
  category, line and column (#1748). Upgrade step: none beyond the
  upgrade; rotate any key or token a log line may have recorded.
