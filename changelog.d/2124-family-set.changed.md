- The gateway moves to the FerroHEALTH family set: `ferrohealth-server`
  0.14.0, `ferrohealth-config` 0.5.0, `ferrohealth-telemetry` 0.6.0,
  `ferrohealth-usage-report` 0.6.0, `ferrohealth-smart` 0.38.2,
  `ferrohealth-session` 0.36.0, `ihe-atna` and `ehds-logging` 0.32.0,
  `ihe-iti` and `nl-generic-functions` 0.30.0, and `fhir-types` 0.5.0 with
  the interoperability component at `eehrxf` 0.52.0, `fhirconnect` 0.16.0,
  `ferrobridge-term` 0.25.0 and `ferrobridge-validator` 0.26.0. Transcoding
  for MyHealth@EU knows every system a binding's value set draws on, so a
  concept coded in one of them that is not a translation target, such as a
  SNOMED CT allergen, is asked of `ValueSet/$validate-code` and translated
  only when it is no member.
