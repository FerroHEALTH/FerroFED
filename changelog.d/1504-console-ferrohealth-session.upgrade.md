- Nothing in an operator console's configuration has to change (#1504). The
  `[oidc]` endpoints are now optional: remove `authorization_endpoint`,
  `token_endpoint` and `jwks_uri` to have the console discover them from
  `issuer` when it starts, which then needs the provider reachable at start.
  A reverse proxy in front of the console passes `/session` through to it.
