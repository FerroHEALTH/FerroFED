- The named refusals of keys v0.0.17 and earlier releases removed or
  renamed are deleted, under the one-release rule for removal aids; each
  such key is now refused as an unknown key. They are `[dev]` (removed in v0.0.14),
  the `[server]` keys v0.0.16 renamed (`server.public_url`,
  `server.replica`, `server.bindings_drain_timeout_ms` and
  `server.overload_retry_after_s`), the outbound TLS keys left on a table
  outside its `tls` sub-table, `[credentials."<id>".oauth2]`, `.fapi2` and
  `mtls_alias_hosts` and their service forms,
  `[trust.issuer.national_contact_point]`, `[trust.issuer.requester]`,
  `[trust.issuer.assurance]`, `client_tokens_act_for_professional` and
  `[stored_queries] url` and `url_file`. The Upgrade notes of the release
  that removed each key give its step. `[[auth.basic.users]]` stays
  refused by name: the gateway admits no Basic caller (#1787).
