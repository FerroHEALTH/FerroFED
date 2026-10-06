- Every write action an operator runs on the admin listener is recorded
  (#767): one line under `ferrofed::security` with `event`
  `admin-write-admitted`, naming the operator's issuer and subject, how it
  was admitted, the method and route template, the status it was answered,
  the time and the request id, and never the token. It is counted under
  `ferrofed_security_events_total` by the same name.
