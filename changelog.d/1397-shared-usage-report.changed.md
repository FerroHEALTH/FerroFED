- The installation report is sent by `ferrohealth-usage-report` 0.2.0, the
  FerroHEALTH family crate, on FerroPULSE's envelope v2; FerroFED's own
  section, envelope, reporter, state directory and window are deleted and
  the gateway supplies only its `status`, its route groups and its node
  counters (#1397). The `[usage_report]` keys and their defaults are
  unchanged, so no configuration changes. A daily report now leaves out a
  counter that counted nothing in its window where it sent `0`, the boot
  line with the report on ends with the family's wording ("no clinical
  content, identifiers, URLs or request paths"), and an `endpoint` the
  collector rule refuses is refused as an invalid value of
  `usage_report.endpoint` while the configuration is read.
