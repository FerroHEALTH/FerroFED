- `[server] site_root` of the operator console, and its
  `FERROFED_CONSOLE__SERVER__SITE_ROOT` override: the console serves no
  site bundle, so the key is an unknown key, which `serve` and
  `config check` refuse by its name (#1512).
