- The patient summary's stored section queries (#776): wherever the
  stored-query registry is offered, it holds eleven read-only queries of the
  gateway's own under the reserved namespace `eu.ferrofed.eehrxf`, at the
  immutable version `1.0.0`, one per patient summary section openEHR
  content feeds (`eu.ferrofed.eehrxf::patient-summary-problems` and so on).
  Each selects, from every member, the whole compositions that contain one
  of the archetypes the openEHR International Patient Summary template names
  for its section, once each, with the composition's uid and template id.
  The patient is bound through `$patient` and `$namespace`, so no node
  receives the identifier. They are listed, read and run by name as any
  stored query. A `PUT` into the namespace answers `409`
  `stored-query-reserved`, and a store that holds a definition there refuses
  the start. The selection, and the four sections no query feeds, are in the
  book's clinical safety risk file, pending the clinical review.
- `app/ferrofed-eehrxf` 0.0.1, the interoperability component's federation
  half, which builds the section queries as AQL syntax trees with
  `openehr-query`; `eehrxf` still links nothing of FerroFED (#776).
- The openEHR International Patient Summary template of the openEHR
  international Clinical Knowledge Manager, vendored with its source under
  CC BY-SA by `scripts/vendor/openehr-ckm.sh` (#776).
