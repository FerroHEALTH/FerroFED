- A national contact point's emergency access is held to the relayed health
  professional's limit (eHDSI Section II Security Services 10.0.0 §2.5; NCPeH
  Architecture Specifications 10.0.0 §5.1.9): five per professional in any
  24 hours by default, tightened with `[emergency] threshold` and
  `window_s`, and a looser limit refuses the configuration naming §2.5. Past
  the limit the request is denied `403 emergency-limit-reached` before any
  member is asked, with `ERROR_SEC_GENERIC` on the FHIR face, an access
  record carrying the emergency mark and the warning
  `EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS`, and the security event
  `emergency-limit-reached`. The count lives in FerroFED's own store, one
  statement per attempt, so it holds across replicas and restarts; with the
  store down an emergency access is refused `503 emergency-uncounted`
  (#1631).
