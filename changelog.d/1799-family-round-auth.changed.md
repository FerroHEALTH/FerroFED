- The gateway runs on `ferrohealth-smart` 0.15.1, `ferrohealth-session`
  0.9.0, `ihe-atna` and `ehds-logging` 0.9.1, `ihe-iti` 0.0.84 and
  `nl-generic-functions` 0.0.51, with FerroBRIDGE's `eehrxf`,
  `ferrobridge-term` and `ferrobridge-validator` 0.2.0, `fhirconnect`
  0.1.249, `openehr-mapping-core` 0.1.151 and the `openehr-*` crates at
  0.0.136. A patient's representative is the caller `ferrohealth-smart`
  classifies as one, and the gateway no longer reads the `act` claim
  itself. A node grant's own `tls` table beside the node section's TLS
  identity is refused, naming both (#1799).
