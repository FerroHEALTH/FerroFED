- The operator console refuses to start with `[session] secure_cookie = false`
  unless its `[oidc] redirect_uri` is an `http` URL on a loopback host
  (`localhost`, `127.0.0.0/8` or `::1`), so a deployed console never sends its
  session cookie over plain HTTP; the refusal names `session.secure_cookie`
  (#615).
