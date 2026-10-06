- The book's data-protection and threat-model pages describe the access log
  as built (#711): the access records are a row of the processing inventory,
  the `log` audit destination row says it names no client of a request the
  gateway received and is refused for a registry outside development, and
  boundary B6 and the deployment's risks name what the access log covers and
  what stays planned. The hardening guide asks for `[audit] destination =
  "repository"` once a registry is configured. The authentication page no
  longer says the open health routes name no member:
  `GET {base}/health/dependencies` names every member endpoint id and its
  state. The operator console page says to set `Strict-Transport-Security`
  at the proxy. The README and `llms.txt` no longer say the European logging
  component is not built.
