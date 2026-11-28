<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-quantum inventory

This page lists every cryptographic use in FerroFED, its console and its
release workflows, the algorithm each uses, and whether it resists a
cryptographically relevant quantum computer. The machine-readable copy is
`conformance/post-quantum.tsv`: after its `#` comment lines, one row per use
with five tab-separated columns: `use`, `algorithm`, `safe` (`yes` or `no`),
`pq_standard` (the standard that defines the post-quantum form, or
`no standard yet`) and `location`. Every change that adds, removes or
changes a cryptographic use updates the file in the same pull request, and
`scripts/checks/post-quantum.sh --update` rewrites the table below and the
README badge from it. CI fails when the table, the badge or a location named
in the file disagrees with the tree.

## The sources

- **NIST IR 8547 (final):** ECDSA, EdDSA and RSA signatures (§4.1.1) and
  ECDH, DH and RSA key establishment (§4.1.2) are deprecated after 2030 and
  disallowed after 2035. Symmetric primitives with at least 128 bits of
  classical security, AES-128 and up, SHA-256 and HMAC-SHA-256 among them,
  stay (§4.1.3). FIPS 203, 204 and 205 define ML-KEM, ML-DSA and SLH-DSA.
- **The EU coordinated implementation roadmap (NIS Cooperation Group,
  v1.1):** a use case is high-risk when its confidentiality must hold for 10
  years or more, which health data needs, and high-risk use cases complete
  the transition by 31.12.2030 (§4.2). After 2030 they use no
  quantum-vulnerable public-key mechanism stand-alone, and standardised
  hybrid solutions are recommended (§4.1).
- **RFC 10024:** the TLS 1.3 hybrid group X25519MLKEM768 (codepoint 0x11EC),
  within the hybrid design of RFC 9954.
- **RFC 9964:** ML-DSA for JOSE and COSE. **RFC 9881:** ML-DSA in X.509.
  **RFC 9980:** post-quantum OpenPGP.

A row is `safe` when every key it depends on is established or verified by a
post-quantum or hybrid mechanism, or when it is a symmetric primitive at 128
bits or more. A row whose post-quantum form has no standard yet counts as not
safe and stays out of the badge, which counts the safe rows of the rows a
post-quantum standard covers; the `no standard yet` rows of the table below
are the rest.

## What FerroFED does

The gateway, admin and console listeners come from `ferrohealth-server`,
which offers X25519MLKEM768 first in the TLS 1.3 key exchange, then X25519,
P-256 and P-384 for a peer that lacks the hybrid group. It enables rustls's
`prefer-post-quantum` feature, and FerroFED's own `rustls` dependency names
it too, so every other rustls user in FerroFED takes the same order from the
default provider: the clients FerroFED builds itself (the node engines, the
Nuts grant, the conformance run and the console's client of the gateway),
the PostgreSQL store, and the family clients built through
`ferrohealth-server` and `ferrohealth-smart`. Tests show the listener and
each client FerroFED builds itself agreeing on X25519MLKEM768 with a peer
that offers it beside X25519, and on X25519 with a peer that offers only
that.

The key exchange protects what a session carries against an attacker who
records it now and decrypts it later, the risk the roadmap gives first
place. The certificate signatures that authenticate a peer stay classical:
no TLS 1.3 signature scheme for ML-DSA is published as an RFC yet, and the
operator's certificates decide the algorithm. The JOSE signatures of the
access tokens, the client assertions, the caller token, the `DPoP` proofs,
the Nuts grants, the document tokens and the console sign-in stay classical
until the issuers, nodes and authorization servers FerroFED works with sign
and verify with ML-DSA (RFC 9964). The document list's attachment hash is
SHA-1 because FHIR R4 `Attachment.hash` defines it so; SHA-1 is below the
128-bit floor of §4.1.3, so the row counts as not safe.

## The inventory

| Use | Algorithm | Safe | Post-quantum standard | Location |
|---|---|---|---|---|
| TLS 1.3 key exchange of the gateway, admin and console listeners | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-server tls, app/ferrofed-server/src/command/serve.rs, app/ferrofed-server/src/admin.rs, app/ferrofed-console/src/server.rs` |
| TLS 1.3 key exchange of the clients FerroFED builds: the node engines, the Nuts grant, the conformance run and the console's client of the gateway | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `app/ferrofed-server/src/onward.rs (node_transport, transport), app/ferrofed-server/src/federation/build.rs, app/ferrofed-server/src/binding/nl/nuts.rs (http_client), app/ferrofed-server/src/conformance/client.rs, app/ferrofed-console/src/gateway/mod.rs` |
| TLS 1.3 key exchange of the family clients: the identity services, the audit repository, the terminology server, the OpenID Provider and the OTLP export | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-server tls_client, ferrohealth-smart client, app/ferrofed-server/src/config/tls.rs, app/ferrofed-identity/src/outbound.rs, app/ferrofed-server/src/config/ihe/audit_repository.rs` |
| TLS 1.3 key exchange of the PostgreSQL store | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `app/ferrofed-server/src/store/connection.rs (client_config)` |
| TLS 1.2 key exchange where min_version lowers the minimum | ECDHE over X25519, P-256 or P-384 | no | no standard yet | `ferrohealth-server tls (protocol_versions)` |
| TLS server and client certificate authentication, mutual TLS to a node included | ECDSA, RSA or Ed25519, as the operator's certificates hold | no | no standard yet | `ferrohealth-server tls, ferrohealth-server tls_client` |
| TLS record protection | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305 | yes | NIST IR 8547 4.1.3 | `ferrohealth-server tls` |
| Access token and DPoP proof verification | RS256, RS384, PS256, PS384, ES256, ES384 | no | RFC 9964 | `ferrohealth-smart resource, app/ferrofed-server/src/auth/verify.rs` |
| Client assertions FerroFED signs (private_key_jwt, FAPI 2.0) | ES256 or ES384 | no | RFC 9964 | `ferrohealth-smart client assertion, app/ferrofed-server/src/config/secrets.rs, app/ferrofed-server/src/config/oauth.rs, app/ferrofed-server/src/jwks.rs` |
| The caller token FerroFED signs for each node | ES256 or ES384 | no | RFC 9964 | `app/ferrofed-engine/src/conveyance/mod.rs` |
| DPoP proofs FerroFED signs | ES256 or ES384 | no | RFC 9964 | `ferrohealth-smart client dpop, app/ferrofed-engine/src/dispatch/dpop.rs` |
| Nuts grants FerroFED signs | ES256 or ES384 | no | RFC 9964 | `app/ferrofed-engine/src/onward/grant/nl/nuts.rs, app/ferrofed-server/src/config/nl/nuts.rs` |
| Signed Binary tokens of the document list | ES256 or ES384 | no | RFC 9964 | `app/ferrofed-server/src/fhir/binary.rs` |
| Console sign-in ID token verification | RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384, EdDSA | no | RFC 9964 | `ferrohealth-smart oidc, app/ferrofed-console/src/config/settings.rs` |
| Patient and document ids of the FHIR face, and identifier sealing | HMAC-SHA-256, AES-256-GCM, HKDF-SHA-256 | yes | NIST IR 8547 4.1.3 | `app/ferrofed-server/src/ids.rs` |
| Caller references in the security log | HMAC-SHA-256 | yes | NIST IR 8547 4.1.3 | `app/ferrofed-server/src/auth/reference.rs` |
| TRC patient pseudonyms | HMAC-SHA-256 | yes | NIST IR 8547 4.1.3 | `app/ferrofed-server/src/facade/trc.rs` |
| Document list attachment hashes | SHA-1, as FHIR R4 Attachment.hash defines | no | no standard yet | `app/ferrofed-server/src/fhir/list.rs` |
| Basic password hashing | Argon2id | yes | NIST IR 8547 4.1.3 | `ferrohealth-smart basic, app/ferrofed-server/src/config/auth` |
| crates.io publishing | Trusted Publishing (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/publish-crates.yml` |
| Release provenance and SBOM attestations | Sigstore attestation (ECDSA P-256) | no | no standard yet | `.github/workflows/release-build.yml, .github/workflows/release-image.yml, .github/workflows/release-console.yml` |
