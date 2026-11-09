- Move `client_identity`, `client_identity_file` and `trust_roots_file`
  of `[xcpd.audit_repository]` into its `tls` sub-table,
  `[xcpd.audit_repository.tls]`, as every other outbound table took them
  with #1750; the sub-table also takes `min_version`, `"1.3"` unless
  lowered to `"1.2"`. This release refuses a key left on the table at
  start-up and in `config check`, naming the `tls` key that replaces it
  ([The audit repository](https://ferrofed.eu/docs/operate/localization.html#the-audit-repository), #1766).
