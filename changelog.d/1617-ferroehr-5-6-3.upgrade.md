- Run `docker compose down -v` once before you start the quickstart on
  this release. FerroEHR 5.6.0 starts its audit store empty and refuses one
  that holds records an earlier release wrote, so the nodes do not start on
  a `ferroehr-pgdata` volume that FerroEHR 5.5.0 wrote (#1617).
