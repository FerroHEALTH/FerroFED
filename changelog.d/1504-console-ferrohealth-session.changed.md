- The operator console signs in and keeps its sessions through
  `ferrohealth-session` 0.1.1, the family's console sign-in, over
  `ferrohealth-smart`'s relying party; its own sign-in, session store and
  code exchange are gone (#1504, #1506). The routes stay `GET /login`,
  `GET /auth/callback` and `POST /logout`, and the cookies keep their names.
- With `[oidc] issuer` alone, the console reads the provider's endpoints from
  its discovery document at start (OpenID Connect Discovery 1.0 §4); a
  missing or unreachable document refuses the start, and `config check`
  never asks the provider. `[oidc]` gains `revocation_endpoint`, `ca_file`
  and `timeout_s`, and `[session]` gains `store`, `key` and `key_file` for a
  console that runs as several instances.
- A sign-out with no `[oidc]` ends the session and lands on `/` in place of
  answering `503`, and a refresh the provider cannot be reached for keeps
  the session for the next request in place of ending it.
