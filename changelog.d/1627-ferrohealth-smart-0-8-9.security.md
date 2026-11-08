- Role claims hold per issuer (GHSA-pfgx-xq28-2629, CWE-863, CWE-639).
  Before it, beside several `[[auth.issuer]]` entries, `[auth] role_claims`
  applied to every trusted issuer's tokens. `ferrohealth-smart` 0.8.9 read
  an entry written `<issuer>#<path>` from that issuer's tokens alone, and
  the 0.9 line this release runs on moves the list onto each
  `[[auth.issuer]]` table as `role_claims`, read from that issuer's tokens
  alone (#1682). Upgrade step: remove `role_claims` from `[auth]`, which now
  refuses the start naming its replacement, and set it on an issuer only
  where that issuer's tokens carry roles (the gateway reads the
  professional's roles from the IUA `subject_role`). `scopeless_subjects`
  stays refused as before.
