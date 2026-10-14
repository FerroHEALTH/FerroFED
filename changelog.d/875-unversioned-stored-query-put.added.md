- A stored-query `PUT {base}/v1/definition/query/{name}` with no version is
  stored as a new version the registry assigns: `1.0.0` for a name it holds
  no version of, else the next patch of the newest version by number (after
  `1.9.0` and `1.10.0`, `1.10.1`), answered `200` with `Location` naming it
  as `./{name}/{version}` (§12.7 `stored-query-versioning`; ITS-REST
  `definition_query_store`). A held version is never replaced: the embedded
  store reads and writes in one transaction, and over PostgreSQL a replica
  that finds the version taken assigns the next, up to eight times, then
  answers `409 stored-query-held` and stores nothing. The literal-patient
  refusal applies before any version is assigned, the reserved namespace
  stays closed, a read-only registry still answers `405`, and a distribution
  sends the assigned version. The code `query-version-required`, the `400`
  this `PUT` answered before, is retired, and the error page lists it as
  retired (#875).
