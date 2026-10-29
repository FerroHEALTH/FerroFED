- The panic line comes from the family's panic hook (`ferrohealth-telemetry`
  0.2.4): its message is `a thread panicked; the message is withheld`, and it
  names the source as `file` and `line` with the `thread`, where FerroFED's
  own hook wrote `a thread panicked` with one `location` field. It still names
  the request by the gateway's `request_id` and never the panic message. A
  log alert that matched the old message or `location` needs the new ones.
