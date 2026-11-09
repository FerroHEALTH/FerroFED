- Every node grant obtains its token through `ferrohealth-smart`'s client,
  and FerroFED's own OAuth 2.0 client is gone (#1749):
  - The token request carries the scope as configured, no longer
    rewritten into the SMART on openEHR canonical form, and a token
    endpoint may carry a query (RFC 6749 §3.2). One with a user name,
    password or fragment is refused.
  - A token with no stated lifetime is reused until the node refuses it,
    and a token shorter than twice the refresh margin is reused for half
    its lifetime. Each send to the token endpoint, the DPoP nonce resend
    included, has the grant's `timeout_ms` of its own.
  - A token exchange runs through the node's onward lane. With no `scope`
    on its `onward` table it asks for the caller's covering scopes and
    refuses a call none covers before anything is sent; a configured
    `scope` is asked for as written. An exchanged token bound with DPoP
    reaches the node under the DPoP scheme, and one the node answers `401`
    is exchanged again.
  - The issuer of a node's `oauth` grant is held to the cleartext policy,
    as its token endpoint is: `https` outside the development profile.
