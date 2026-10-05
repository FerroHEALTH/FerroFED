- `[signing] next_key_file`, a signing key the JWK Set at
  `{base}/.well-known/jwks.json` publishes ahead of a rotation and that never
  signs, so every replica publishes a new key before any replica signs with
  it (#627). `next_key_algorithm` names the algorithm the next key is meant
  for, and `config check` refuses a next key on another curve, naming
  `signing.next_key_file`, or one that is already the current or previous
  key. The book gives the three-step rotation for any number of replicas
  and the node cache time it assumes.
