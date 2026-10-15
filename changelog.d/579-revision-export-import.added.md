- Under `config.ui = "managed"`, any configuration revision exports as TOML
  and TOML imports as a dry-run revision (#579).
  `GET /admin/config/revisions/{id}/export` answers the revision as an
  attachment under a comment header naming it, its author and its time,
  with every secret as its `_file` reference and without the bootstrap core.
  `POST /admin/config/import?base=&reason=` takes a TOML body of at most
  1 MiB, checks it as a save, refuses a change to a resource the file
  provides `409 resource-file-owned`, and saves it unapplied with the origin
  `import`, answering its changes against the running revision; you apply it
  as any revision. The Configuration page offers each revision of its
  history as a download and an import form that shows the dry run's
  changes. The book describes the file-first and console-first ways of
  working and how to move between them.
