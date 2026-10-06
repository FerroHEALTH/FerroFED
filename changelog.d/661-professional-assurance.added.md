- The authentication assurance of every caller that reaches patient data
  (#661, Regulation (EU) 2025/327 Annex II 3.1). Per issuer,
  `[auth.issuer.assurance]` names the claim that carries the assurance,
  `acr` by default (RFC 9068 §2.2.1), the values that stand for the levels
  `low`, `substantial` and `high` of Regulation (EU) No 910/2014 Art 8(2),
  and the least level a query, an EHR request or a DEMOGRAPHIC request
  needs. A token below it, without the claim, or with a value listed at no
  level is answered `401 authentication-assurance-insufficient` with the
  RFC 9470 §3 challenge `insufficient_user_authentication`; no node is
  asked and no access record is written. The level is read from the token
  or the introspection answer, and `config check` notes each issuer that
  declares none. Each node is told who acts (`acting`), the level reached
  (`assurance_level`) and the professional's IHE IUA `subject_name` and
  `national_provider_identifier` in the caller's
  `openEHR-federation-client` token, and in nothing else the gateway sends.
