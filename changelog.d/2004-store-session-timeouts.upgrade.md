- `[store]`: the gateway sends the libpq `options` startup parameter on
  every connection. A connection pooler in front of the store must pass it
  on, or the gateway must connect to PostgreSQL itself.
