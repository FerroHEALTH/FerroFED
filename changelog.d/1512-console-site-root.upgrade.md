- Remove `[server] site_root` and `FERROFED_CONSOLE__SERVER__SITE_ROOT`
  from the operator console's configuration: the console serves no site
  bundle, and this release refuses the key at start-up and in
  `config check`, naming it (#1512).
