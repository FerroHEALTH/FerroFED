- Move each table's outbound TLS keys, `client_identity`,
  `client_identity_file` and `trust_roots_file`, into its `tls` sub-table:
  `[[pixm.manager]]` to `[pixm.manager.tls]`, and likewise `[pdqm.tls]`,
  `[pmir.tls]`, `[registry.mcsd.tls]`, `[xcpd.tls]`, `[nl_gf.nvi.tls]`,
  `[nl_gf.mitz.tls]`, `[audit.repository.tls]`, `[eehrxf.terminology.tls]`
  and `[credentials."<endpoint id>".tls]`. This release refuses a key left
  on its table at start-up and in `config check`, naming the `tls` key that
  replaces it
  ([Mutual TLS to the identity services](https://ferrofed.eu/docs/operate/configuration.html#mutual-tls-to-the-identity-services),
  #1750).
- The gateway speaks TLS 1.3 alone to every outbound service unless the
  table's `tls` sub-table lowers it with `min_version = "1.2"`. Set it on
  `[xcpd.tls]` when a MyHealth@EU responding gateway speaks only TLS 1.2,
  the minimum eHDSI Section II Security Services 10.0.0 §5.9.1 and eHDSI
  Cryptographic Algorithms 9.0.0 admit between NCPeH, and on any other
  table whose service speaks no TLS 1.3
  ([A MyHealth@EU route](https://ferrofed.eu/docs/operate/localization.html#a-myhealtheu-route),
  #1750).
