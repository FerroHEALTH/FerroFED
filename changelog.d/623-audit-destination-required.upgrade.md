- A gateway with a registry outside the development profile must set
  `[audit] destination` (#623, #693), and it must be `"repository"` with an
  `[audit.repository]` table: `"log"` is refused there, because it records no
  caller and no patient and so cannot hold the access log Regulation (EU)
  2025/327 Annex II 3.2 asks for. Point the gateway at your Audit Record
  Repository, with `spool_dir` on durable storage, before you upgrade.
