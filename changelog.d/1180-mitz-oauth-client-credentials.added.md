- Mitz takes the OAuth 2.0 client credentials grant of
  `[nl_gf.mitz.credentials.oauth]` beside basic credentials. The gateway
  obtains the token at the token endpoint, which must be `https` outside
  `profile = "development"`, caches it while it is fresh, and sends it with
  each closed authorization question as a bearer token; a question Mitz
  answers `401` is asked once more with a fresh token. An `oauth2`, `nuts`
  or `fapi2` grant is still refused at load (#1180).
