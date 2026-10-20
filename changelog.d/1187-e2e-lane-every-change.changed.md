- The `e2e-placement` guard now refuses a CI workflow that could skip the
  container tests: a path filter on its triggers, a condition on the
  `e2e (containers)` job beyond the workspace check, or a `conclusion` check
  that does not wait for it. A change to the auth, credentials or
  configuration code, an e2e fixture or the configuration reference always
  runs the container tests (#1187).
