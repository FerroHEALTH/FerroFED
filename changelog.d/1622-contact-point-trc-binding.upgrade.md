- `[trust.issuer.national_contact_point]` requires `trc_patient`, the claim
  your contact point's authorization server writes the patient of the
  treatment relationship confirmation in, as `system|value`. A table without
  it fails `config check`, naming the key (#1622).
