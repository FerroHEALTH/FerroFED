- `[auth] role_claims` holds per issuer, through `ferrohealth-smart` 0.8.9
  (GHSA-pfgx-xq28-2629, CWE-863, CWE-639). Before it, beside several
  `[[auth.issuer]]` entries the list applied to every trusted issuer's
  tokens. An entry is now written `<issuer>#<path>` and read from that
  issuer's tokens alone, and beside several issuers a bare entry refuses the
  start. Upgrade step for a gateway with several issuers: set
  `role_claims = []` under `[auth]` (the gateway reads the professional's
  roles from the IUA `subject_role`, never from this list), since the
  default `role_claims` is bare and refused. `scopeless_subjects` stays
  refused as before. A gateway with one issuer changes nothing.
