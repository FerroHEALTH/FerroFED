- The quickstart `compose.yaml` switches FerroFED's own usage report off,
  as it already did for the FerroEHR nodes, so a local stack no longer
  counts as an installation; `scripts/checks/usage-report-off.sh` now reads
  the quickstart compose file too (#1280).
