- FerroFED runs on `ferrohealth-server` 0.4.1, which keeps a product's own
  fallback and the layers around it when the stack merges its routes
  (FerroHEALTH/FerroSYS#199). FerroFED was not exposed to the regression
  0.4.1 fixes: the client surface and the FHIR face declare their own
  fallback inside the client authentication gate, so an anonymous request
  to a path no route matches under `/v1/` or `/operator/` is refused with
  `401` before and after.
