- With `[stored_queries] backend = "postgres"`, move `url` or `url_file`
  from `[stored_queries]` to a new `[store]` table, pointed at the database
  the backend uses now, and set `sslmode=require` in the connection string,
  with `ca_file` when the database's CA is not one of the platform's roots.
  This release refuses `[stored_queries] url` and `url_file` at start-up and
  in `config check`, naming each, and a `[store]` without
  `sslmode=require` under every profile. The gateway takes over the
  existing table with every definition it holds
  ([FerroFED's own store](https://ferrofed.eu/docs/operate/store.html#moving-from-stored_queries-url),
  #1636).
- A gateway with a national contact point (`[[federation.contact_point]]`)
  needs `[store]` too: give it a PostgreSQL database and role of its own,
  never a CDR's. Without it this release refuses to start, naming
  `federation.contact_point[0]` and `[store]` (#1636).
- A binary you build yourself no longer takes `--features postgres`; drop
  the flag (#1636).
