- A request that reaches patient data needs a natural person behind its
  token (#661, Regulation (EU) 2025/327 Annex II 3.1). A token whose `sub`
  is its `client_id` (IHE IUA ITI TF-2 3.71.4.2.2.1), or one only `system/`
  scopes cover (SMART on openEHR master08), is answered
  `401 natural-person-required`, unless its issuer declares
  `client_tokens_act_for_professional = true` and the token names the
  professional, in the IUA `national_provider_identifier` or the
  `[auth.issuer.requester]` professional claim. Definitions, `OPTIONS` and
  the operator surface are unchanged.
