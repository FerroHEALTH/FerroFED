- Every write action an operator runs on the admin listener is recorded
  under `ferrofed::security` (#767): `admin-write-admitted` before the
  action has any effect, counted under the same name, and
  `admin-write-finished` with its outcome, `abandoned` when it never
  answered. Each names the operator's issuer and subject, the method and
  route template, and the time, and nothing else: never the token, a path
  value, a body or a stored query, and the issuer and subject are never a
  metric label.
