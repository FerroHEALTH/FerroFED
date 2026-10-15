- Configuration revisions (#576): `config.ui = "managed"` keeps the
  configuration, less its file-only bootstrap core (`profile`, `[server]`,
  `[metrics]`, `[auth]`, `[config]` and `[dev]`), as a history of revisions
  in an embedded `redb` file or a PostgreSQL database the replicas share.
  The file load is a revision, and an operator saves, compares, applies
  through the reload path and rolls back revisions on the admin listener
  under `/admin/config`, behind the issuer's operator scope. Each revision
  is checked as `config check` checks the file and records who, when and
  why; a save on a stale base answers `409 revision-stale`; a secret is
  held only as its `_file` reference. `config.ui = "off"`, the default,
  changes nothing.
