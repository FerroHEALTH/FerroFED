- Every request to a node now tells it the professional's roles, from the IHE IUA
  `subject_role`, and the agency that issued the professional's identifier, from the
  claim an issuer's new `professional_issuing_authority` names, beside the
  identifier and the provider's identifier it already carried, so a node can
  identify the professional and provider of every registration (Regulation (EU)
  2025/327 Art 13(4), Implementing Regulation (EU) 2026/2099 Annex Table 1). The
  outbound gate reads the new claims as it reads every caller claim (#662).
