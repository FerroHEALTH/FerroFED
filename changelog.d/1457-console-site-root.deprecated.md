- `[server] site_root` of the operator console, and its
  `FERROFED_CONSOLE__SERVER__SITE_ROOT` override: the console serves no
  site bundle, so the key does nothing. `config check` and the start-up log
  warn while it is set, and release 0.0.17 refuses it; remove it from your
  console configuration (#1457).
