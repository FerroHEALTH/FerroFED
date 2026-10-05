- The IHE FHIR services take the OAuth 2.0 client-credentials grant (RFC 6749
  §4.4; IHE IUA ITI-71) in `[pixm.manager.credentials.oauth2]`,
  `[pdqm.credentials.oauth2]`, `[pmir.credentials.oauth2]` and
  `[registry.mcsd.credentials.oauth2]` (#677). The gateway authenticates with
  a client secret, in the Basic scheme (`client_auth = "client_secret_basic"`,
  as ITI-71 prescribes) or the request body (`"client_secret_post"`), read
  inline or from `client_secret_file`, or with a client assertion the
  `[signing]` key signs (`"private_key_jwt"`), and asks for the `scope` the
  service's authorization server defines. The token is cached until 30
  seconds before it expires and incorporated in each request as a bearer
  token (ITI-72); a `401` gets one more request with a fresh token. `config
  check` names every key it refuses: token exchange, a `DPoP` key, a
  certificate binding or TLS client authentication in an identity service's
  grant, a secret beside `private_key_jwt`, a scope outside RFC 6749 §3.3,
  and a token endpoint that is not `https` outside the development profile.
  A node's `oauth2` grant refuses a client secret. The `ihe-iti` PIXm,
  PDQm, mCSD and PMIR clients take an `authorizer::Authorizer` for the
  headers of each request.
