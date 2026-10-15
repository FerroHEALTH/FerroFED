- `ferrofed_node_requests_total` carries a new label, `class`: `member`
  for a member's own answer or failure, `client-error` for a `4xx` it
  answered a routed request with. Such a `4xx` now counts as
  `outcome="node-error"`, where it was `active`. The shipped
  `FerroFEDMemberFailing` rule leaves `class="client-error"` out, so it
  pages on the same member failures as before; a rule or a panel of your
  own that reads `outcome="node-error"` should leave it out too (#982).
