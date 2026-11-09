- `[xcpd.audit_repository]` reads its TLS material through
  `ferrohealth-server`'s `TlsClient`, as every other outbound table does,
  and reaches the ITI-20 syslog repository through the platform's verifier
  with the extra trust roots, at TLS 1.3 unless `min_version` lowers it
  ([The audit repository](https://ferrofed.eu/docs/operate/localization.html#the-audit-repository), #1766).
