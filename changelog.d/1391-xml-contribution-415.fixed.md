- A `CONTRIBUTION` sent as `application/xml` is answered `415`
  (`media-type-unsupported`) before it is routed, naming the media types
  `contribution_create` takes, as a FerroEHR node answers it: ITS-REST 1.1.0
  defines that body in JSON alone. It was a `400`
  (`preceding-version-invalid`) after the write was routed (#1391).
