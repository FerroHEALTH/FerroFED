- A DEMOGRAPHIC request now goes to the endpoint
  `federation.demographic_endpoint` declares without an
  `openEHR-federation-endpoint` header (#873). One destination the operator
  fixes and `OPTIONS {base}/` declares is the explicit choice §12.6 asks
  for, so the request no longer answers `400` `target-required`. A header
  naming another endpoint is still `400` `targeting-conflict`, the query
  string and headers are held as before, and `its_rest.demographic` says the
  header is optional.
