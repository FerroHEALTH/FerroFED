- Move the address, TLS and credential of every identity, localization and
  consent service into its `endpoint` table, `ferrohealth-smart`'s outbound
  table, as `[audit.repository]` and `[eehrxf.terminology]` moved: `url`,
  `tls` and `credentials` of `[[pixm.manager]]`, `[pdqm]`, `[pmir]`,
  `[registry.mcsd]`, `[nl_gf.nvi]` and `[nl_gf.mitz]` go to
  `[<table>.endpoint]`, and the `timeout` of `[pdqm]`, `[pmir]` and
  `[nl_gf.mitz]` (from `timeout_ms`) to `endpoint.timeout`. Each
  `[[xcpd.gateway]]` takes its own `[xcpd.gateway.endpoint]` with its `url`,
  and the shared `[xcpd.tls]` moves into the `tls` of each gateway's
  endpoint table. A table you leave partly written keeps its defaults: one
  try per call, `1s` for PDQm and Mitz, `5s` for PMIR and `10s` for the
  others. The NVI's Nuts grant stays in `[nl_gf.nvi.nuts]`. Each old key
  refuses the start and `config check`, naming its replacement (#1815).
