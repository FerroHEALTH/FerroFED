- If a backend client reads patient data through the gateway with a
  `system/` scope or a token whose `sub` is its `client_id`, set
  `client_tokens_act_for_professional = true` on its `[[auth.issuer]]` and
  have its tokens name the professional it acts for in
  `extensions.ihe_iua.national_provider_identifier` (#661). Until both
  hold, such a request is answered `401 natural-person-required`. Then set
  `[auth.issuer.assurance]` on each issuer whose tokens reach patient data;
  without it no assurance level is required, and `config check` notes the
  issuer.
