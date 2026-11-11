- An imaging report receiver (`[[fhir.receive]]` of the imaging category)
  needs FerroBRIDGE's imaging report template at v0.0.24, which slots the
  CKM `CLUSTER.organisation.v1`, and that archetype among its archetypes:
  the `eehrxf` 0.6.6 imaging mapping does not compile against the earlier
  revision, and the gateway refuses to start with it. Both are vendored under
  `docs/specs/ferrobridge-imaging/`.
