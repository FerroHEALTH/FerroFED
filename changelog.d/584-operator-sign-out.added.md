- Operator sign-out in the operator console (#584). The navigation bar's
  "Sign out" posts to `POST /logout`, which ends the server-side session,
  removes the session cookie and, where the new `[oidc]` key
  `end_session_endpoint` names the provider's end-session endpoint, sends
  the browser there with the ID Token hint, the client id and the optional
  `post_logout_redirect_uri` (OpenID Connect RP-Initiated Logout 1.0). The
  sign-out and every server function the views and the query console call
  are taken only from the console's own pages (`Sec-Fetch-Site`, `Origin`
  or `Referer`); any other request is a `403` before the session is read.
