- `OPTIONS {base}/` now declares that a stored query invoked by name,
  `GET` or `POST {base}/v1/query/{name}[/{version}]`, answers `501 Not
  Implemented` on a gateway without the stored-query registry, as N32
  requires of an unsupported area (#871). With the registry, `its_rest.query`
  says those paths are served from it. The behaviour is unchanged.
