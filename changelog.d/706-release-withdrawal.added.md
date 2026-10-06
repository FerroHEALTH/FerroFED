- `scripts/release/withdraw.sh` withdraws a non-conforming release without
  editing anything published. It refuses a version with no finding row in the
  register of non-conforming versions. It moves the `<major>.<minor>` and
  `latest` image tags to the correcting release and keeps the version's own
  tag and digest. It publishes the advisory, writes the upgrade note, lists
  the version under "Withdrawn versions" in `SECURITY.md` and records the
  withdrawal in the register. It runs dry by default. `docs/release.md`
  describes the route.
