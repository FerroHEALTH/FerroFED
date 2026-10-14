- The registry document refuses an `[[organisation]]` that manages no
  `[[endpoint]]` (§8.1: "an Organization manages 1..* Endpoints"), so
  `config check` and the gateway's start fail on it, naming the
  organisation (#874). Give the organisation an endpoint it manages, or
  remove it. A registry read from an mCSD directory or in the FHIR form
  keeps such an organisation, and selecting it still answers `404` with
  `no-destination`.
