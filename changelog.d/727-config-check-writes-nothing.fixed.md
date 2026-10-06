- `config check` writes nothing to disk (#727). It created the
  `[audit.repository]` and `[xcpd.audit_repository]` spool directories, so it
  failed on a read-only root filesystem and left a directory behind on a
  host. It now checks each spool directory where it stands: one that exists
  must give its owner write access and no one else any, and hold no file the
  gateway did not write; for one that does not exist, the nearest directory
  above it must admit a writer. Only modes are read, and `serve` still
  creates the directory. A spool `serve` would refuse is refused by the key
  that names it, and `serve` names that key too. The production guide's
  check runs on a read-only root with no durable volume.
