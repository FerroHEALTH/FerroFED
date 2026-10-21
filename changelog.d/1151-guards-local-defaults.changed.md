- `scripts/checks/changelog-guard.sh` and `crate-version-guard.sh` take
  `origin/main` as the base when run with no refs, and `stray-literals.sh`
  reads the console's sources when given no directory, so a local run of the
  guards passes on a clean branch; `docs/ci-cd.md` names the guards that need
  a built input first (#1151).
