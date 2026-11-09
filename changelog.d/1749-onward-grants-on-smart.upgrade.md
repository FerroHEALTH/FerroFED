- Move every `[credentials."<id>".oauth2]` and `[credentials."<id>".fapi2]`
  table to `[credentials."<id>".oauth]`, and the section's
  `mtls_alias_hosts` into it. This release refuses each removed key at
  start-up and in `config check`, naming the `oauth` key that replaces it
  ([Onward credentials](https://ferrofed.eu/docs/operate/onward-credentials.html), #1749):
  - The grant's key moves from `[signing]` to the `oauth` table's own
    `private_key_file`, with `previous_private_key_file` and
    `next_private_key_file` for a rotation. The gateway publishes it in its
    JWK Set beside the `[signing]` keys. Pointing `private_key_file` at
    the `[signing]` key file keeps the `kid` the node's authorization
    server knows; a new key needs its `kid` registered there, or the JWK
    Set fetched again, before you switch.
  - `grant = "token_exchange"` becomes an `onward` table beside the `oauth`
    table, with `kind = "persons"`, the exchanged tokens' `issuer`,
    `audiences` and `indicators` naming the node in place of `resource`,
    `actor_assertion = true`, and `[onward.keys] max_onward_lifetime`. The
    node's authorization server must issue a JWT access token that names
    that issuer, an `aud` among `audiences`, and the gateway as `act` or
    client; an opaque exchanged token now fails the node.
  - A `fapi2` table becomes an `oauth` table with
    `security_profile = "fapi2"`, `client_auth = "private_key_jwt"`,
    `assertion_audience = "issuer"`, `issuer`, and `client_key_file` and
    `previous_client_key_file` renamed `private_key_file` and
    `previous_private_key_file`.
  - `tls_client_certificate_bound_access_tokens = true` becomes
    `certificate_bound = true`. A grant that authenticates by, or binds its
    tokens to, the TLS client certificate presents the node section's
    `tls` identity; `client_certificate_file`, `client_key_file` and
    `ca_file` in its `oauth` table are refused.
- Give every node's `oauth` grant `client_auth = "private_key_jwt"`,
  `"tls_client_auth"` or `"self_signed_tls_client_auth"`. A node's grant
  that names `client_secret_basic`, `client_secret_post`, `client_secret`
  or `client_secret_file`, or leaves `client_auth` unset, is refused,
  naming the key (§13.1, N25). Identity service grants keep their client
  secret (#1749).
- Remove `signing.assertion_lifetime_s`: no `[signing]` key signs a client
  assertion any more, and every assertion lives 300 seconds. A file that
  sets it is refused at start-up, naming it. `signing.rotation_overlap_s`
  must be at least 300 plus `signing.node_jwks_cache_s` (#1749).
