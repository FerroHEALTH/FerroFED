- The store's readiness check no longer logs a warning on every failed
  probe while the database is down. It logs the change to down once, a
  reminder at most every five minutes while the store stays down, and one
  line, with how long the outage lasted, when the store answers again
  (#1644).
