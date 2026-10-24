- The admin listener answers the running configuration at
  `GET /admin/config/running`, behind the operator scope: each
  deployment-wide setting with the value the replica runs, where it comes
  from (the file, an environment override, the running revision or the
  default), its default, and whether a change waits for the next start,
  with the value that start takes. A secret is named by its file, never by
  its value. Under `config.ui = "managed"`, `PUT /admin/config/settings`
  saves a revision with named settings changed, applied as any revision
  (#1235).
