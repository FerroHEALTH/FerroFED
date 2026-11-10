- The named refusals that shipped in v0.0.18 for one release are deleted,
  under the one-release rule for removal aids. Each such key is now refused
  as an unknown key: the outbound TLS keys left on a table outside its
  `tls` sub-table, `[credentials."<id>".oauth2]`, `.fapi2` and
  `mtls_alias_hosts` and their service forms,
  `[trust.issuer.national_contact_point]`, `[trust.issuer.requester]`,
  `[trust.issuer.assurance]`, `client_tokens_act_for_professional`, and
  `[stored_queries] url` and `url_file`. The Upgrade notes of v0.0.18 give
  each step. The refusals this release adds for the keys it moves stay
  until v0.0.20 (#1787).
