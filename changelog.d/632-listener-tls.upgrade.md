- The listeners need no change: unset, `[server.tls]` and `[metrics.tls]`
  leave them on plain HTTP as before (#632). Once you set either, remove it
  before rolling back to an earlier release, which refuses it as an unknown
  key.
