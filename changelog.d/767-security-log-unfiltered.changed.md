- `[telemetry] filter` no longer quiets the security log (#767): the
  gateway adds `ferrofed::security=info` after the configured filter, so
  every security line at `info` or above is written whatever it says.
