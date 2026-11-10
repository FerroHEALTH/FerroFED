- Move a node's `user`, `password`, `password_file`, `oauth` and `onward`
  from `[credentials."<id>"]` into its nested
  `[credentials."<id>".credentials]` table, `ferrohealth-smart`'s
  credentials table, with the `FERROFED__CREDENTIALS__<ID>__CREDENTIALS__`
  overrides to match. `tls`, `nuts` and `onward_organisations` stay in the
  section. Each old key refuses the start and `config check`, naming its
  replacement. The operator console's credentials form writes the nested
  table (#1806).
- Move the NVI's Nuts grant from `[nl_gf.nvi.credentials.nuts]` to
  `[nl_gf.nvi.nuts]`; `[nl_gf.nvi.credentials]` is `ferrohealth-smart`'s
  credentials table, which refuses `nuts` as an unknown key (#1806).
