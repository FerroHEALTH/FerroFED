- FerroFED moves to `fhir-types` 0.2 with the family crates built on it:
  `eehrxf` 0.30 with `fhirconnect` 0.9, `openehr-mapping-core` 0.6,
  `ferrobridge-term` 0.19 and `ferrobridge-validator` 0.20, `ihe-iti` and
  `nl-generic-functions` 0.22, and `ehds-logging` and `ihe-atna` 0.27, so one
  copy of each links. Every report the gateway writes now states its language
  on the `Composition`, which MyHealth@EU HDR 9.2.0 requires (#1979).
