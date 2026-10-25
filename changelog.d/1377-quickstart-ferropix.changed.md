- The compose quickstart resolves every patient through FerroPIX, the
  FerroHEALTH patient index, which Compose runs beside the gateway on its own
  PostgreSQL and seeds with the synthetic patients over ITI-104 before the
  gateway starts. The gateway keeps no cross-reference of its own. The
  patients now live in `docker/quickstart/patients.tsv`, which
  `scripts/quickstart/seed.sh` reads too. Run `docker compose down -v` once
  before bringing an older quickstart up again (#1377).
