- `server.public_url` names the gateway's public base URL once (#723). Set,
  an unset `auth.audience` is that URL, an unset `signing.jwks_uri` is the
  JWK Set route under it, and an unset `pmir.callback_url` is the feed route
  under it. Its path must be `server.base_path`, and a `signing.jwks_uri` or
  `pmir.callback_url` that is still written must name the route the gateway
  serves under it, or `config check` refuses it, naming the key and the URL
  it expects. Unset, every key is read as written, as before. The
  production guide shows the shorter form.
