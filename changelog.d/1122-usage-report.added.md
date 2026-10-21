- The gateway sends the FerroHEALTH installation report to FerroPULSE on its
  envelope v2: one `start` report per process start and one `daily` report a
  day, carrying a random instance id, the version, the deployment kind,
  coarse host size, the member count as a range, the configured identity and
  consent services, latency per route group and the node request counts, and
  no patient data, AQL, path, node name or URL. It is on by default once
  `[usage_report] state_dir` names a writable directory, which the release's
  compose file and the Kubernetes example set; `enabled = false`
  (`FERROFED__USAGE_REPORT__ENABLED=false`) stops it, a CI environment stops
  it, and one boot line says which. The book's
  [usage report page](https://ferrofed.eu/docs/operate/usage-report.html)
  lists every field and carries the privacy notice (#1122).
