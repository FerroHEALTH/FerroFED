- `ctutils` moves from 0.4.2 to 0.4.4 (RUSTSEC-2026-0385): before 0.4.4 its
  constant-time mask generation and `u128` comparisons could compile to a
  branch on a secret value. It reaches the gateway through `digest` and the
  Argon2 password hashing of `ferrohealth-smart` (#1232).
