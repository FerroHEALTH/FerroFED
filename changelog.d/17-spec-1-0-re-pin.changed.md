- FerroFED implements the 1.0.0 release of the Federation Tier with AQL
  specification, approved by the openEHR SEC on 2026-10-12, in place of the
  0.9.0 release candidate (#17). `OPTIONS {base}/` now reports
  `federation.spec_version` `"1.0"` (§7a.2), and the startup banner names
  Federation Tier 1.0.0. A client that matches `"0.9"` must accept `"1.0"`;
  no other wire structure changes, and no requirement or conformance point
  moved. `openehr-federation` 0.0.45 carries the new `FEDERATION_SPEC`. The
  book links the specification site at its `1.0` path.
