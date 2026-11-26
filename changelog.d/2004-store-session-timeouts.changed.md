- Every connection to FerroFED's own PostgreSQL store now runs under
  `[store] timeout` on the server too: `statement_timeout`, `lock_timeout`
  and `idle_in_transaction_session_timeout` are set to it at connect, and
  `application_name` to `ferrofed`, so a statement the gateway stopped
  waiting for holds no lock past the budget. A pooler in front of the
  database must pass the libpq `options` parameter on
  ([The store](https://ferrofed.eu/docs/operate/store.html), #2004).
