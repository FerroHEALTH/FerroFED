- The quickstart `compose.yaml` sends the usage report by default, as every
  FerroHEALTH compose file does, with its instance id on a new
  `ferrofed-state` volume: `FERROFED__USAGE_REPORT__ENABLED=false`, and
  `FERROEHR__USAGE_REPORT__ENABLED=false` for the four nodes, in your shell
  or `.env` stop it. `scripts/checks/usage-report-off.sh` holds test lanes
  alone again and no longer reads the compose file (#1296).
