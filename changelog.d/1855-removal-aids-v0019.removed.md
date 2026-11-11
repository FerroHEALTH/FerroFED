- The named refusals that shipped in v0.0.19 for one release are deleted,
  under the one-release rule for removal aids. Each such key is now refused
  as an unknown key: the unit-suffixed keys (`federation.per_node_timeout_ms`,
  `emergency.window_s`, `store.sweep_interval_s` and the rest, #1800), the
  `url`, `credentials`, `tls` and `timeout_ms` keys of every outbound table
  that moved under its `endpoint` table (#1805, #1815), a node's `user`,
  `password`, `password_file`, `oauth` and `onward` outside its nested
  `[credentials."<id>".credentials]` table (#1806), and
  `[trust.issuer.patient] act_is_representative`. The Upgrade notes of
  v0.0.19 give each step (#1855).
