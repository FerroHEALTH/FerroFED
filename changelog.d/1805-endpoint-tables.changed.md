- The gateway runs on `ferrohealth-smart` 0.16.2, `ferrohealth-session`
  0.10.0, `ihe-atna` and `ehds-logging` 0.10.0, `ihe-iti` and
  `nl-generic-functions` 0.1.0, and FerroBRIDGE's `eehrxf`,
  `ferrobridge-term` and `ferrobridge-validator` 0.3.0. The audit
  repository and the terminology server are reached through
  `ferrohealth-smart`'s outbound client, so `[eehrxf.terminology]` also
  takes a client credentials grant. A node's grant presents the node's TLS
  identity through `ferrohealth-smart`'s own form of it
  (`resolve_sharing_tls`) (#1805, #1806).
