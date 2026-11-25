- FerroFED moves to `fhir-types` 0.2 and the `openehr-*` 0.5 line with the
  family crates built on them: `eehrxf` 0.35 with `fhirconnect` 0.13,
  `openehr-mapping-core` 0.9, `ferrobridge-term` 0.19 and
  `ferrobridge-validator` 0.20, `ihe-iti` and `nl-generic-functions` 0.22,
  and `ehds-logging` and `ihe-atna` 0.27, so one copy of each links. Every
  report the gateway writes now states its language on the `Composition`,
  which MyHealth@EU HDR 9.2.0 requires (#1979).
