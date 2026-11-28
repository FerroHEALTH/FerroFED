- ML-DSA (RFC 9964) for the JOSE signatures FerroFED makes and checks with
  its peers, chosen per peer: an issuer's `algorithms` and `[auth.dpop]
  algorithms` admit ML-DSA-44, ML-DSA-65 and ML-DSA-87 tokens and proofs; an
  `oauth` grant's `private_key_file` and `dpop_key_file` take an ML-DSA key
  for its client assertions and `DPoP` proofs, the assertion key published in
  the JWK Set; and `[signing] ml_dsa_key_file` publishes an ML-DSA key that
  signs the caller token of each node whose `[credentials."<endpoint id>"]`
  sets `conveyance_ml_dsa = true`. The post-quantum inventory splits each of
  these uses into its ML-DSA and its classical row.
