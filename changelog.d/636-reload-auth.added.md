- `[auth]` and the listeners' certificates reload without a restart
  (#636): `SIGHUP`, and the new admin listener action `POST /admin/reload`
  for a container no signal reaches, read the configuration, `[auth]` and
  the `[server.tls]` and `[metrics.tls]` certificate files again and apply
  them together or not at all. An issuer a reload adds is trusted from the
  next request on, on the client face and the admin listener alike; a
  request already being admitted finishes on the issuers it started with.
  A reload whose `[auth]` does not load, trusts no issuer of a federating
  gateway (class `auth`), or whose certificate files do not form a pair
  (class `listener-certificate`) is refused as a whole. `auth.audience`
  waits for a restart. `POST /admin/reload` needs the operator scope, is
  recorded with its operator, and answers what it applied, or `422`
  (`configuration-invalid`) naming the refusal's class.
