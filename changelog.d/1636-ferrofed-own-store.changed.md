- The `postgres` backend of the stored-query registry keeps its definitions
  in FerroFED's own store and reads its connection from `[store]`.
  `[stored_queries] url` and `url_file` are removed: a configuration that
  sets either refuses to start, naming the key and `[store]`. The
  definitions' table is the store's first migration, so a database the
  backend already uses keeps every definition. The PostgreSQL client is part
  of every build, and the `postgres` feature of `ferrofed-server` is gone
  (#1636).
