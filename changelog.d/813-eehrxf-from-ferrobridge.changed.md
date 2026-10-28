- **The interoperability component is the published `eehrxf` 0.1.128 from
  FerroHEALTH/FerroBRIDGE**, pinned exactly, in place of the crate this
  repository built (owner decision of 2026-10-28, decision A76). Every change
  to the component is made in FerroBRIDGE, and its pages are FerroBRIDGE's
  book (https://ferrobridge.eu/docs/evaluate/interoperability-component.html);
  this book keeps the gateway's routes, its section queries and the access
  record. `fhirconnect` moves to 0.1.204, `openehr-mapping-core` to 0.1.135,
  `ferrobridge-term` and `ferrobridge-validator` to 0.1.141 and the
  `openehr-*` family to 0.0.109, the versions the component requires. A
  resource the patient summary maps from a composition now takes the
  FHIRconnect deterministic id of the composition uid and its entry path
  (FHIRconnect v1.0.0, engine/id-management.adoc, Deterministic IDs). The
  extension definition the site serves at
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` is
  read from the published crate.
