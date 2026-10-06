- Each access record states how long it is kept (Regulation (EU) 2025/327
  Art 9(2), Annex II 3.4): `[access_log.retention]` declares whole years
  for every record, per category and per registry endpoint, each at least
  three, and a record is kept for the longest its categories and origins
  call for, an unclassified one for the longest declared anywhere. The BALP
  record carries `ehds-retention-years`, `ehds-retention-ends` and
  `ehds-retention-ground`, so the Audit Record Repository can apply it. A
  period under three years, an unknown category and an endpoint the
  registry does not hold are refused when the configuration loads. The
  book's audit page names the ITI-81 searches the log is read with at the
  repository (#795, #660).
- `crates/ehds-logging` 0.0.7: the `retention` module, the period a record
  is kept by category and origin with the three-year floor of Art 9(2),
  the record's `retention`, its three BALP `detail` entries, and
  `CategoryMap::category` (#795).
