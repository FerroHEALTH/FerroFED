- The engine compiles no FHIR model (#730). `nl-generic-functions` 0.0.21 moves
  the request authorizer a Generic Functions client takes out of the
  FHIR-bearing `nvi` feature into a feature of its own, `authorizer`, at
  `nl_generic_functions::authorizer`; `nvi` turns it on, and the engine's
  Nuts grant implements it with `nuts-auth` and `authorizer` alone. The
  architecture test now fails when the engine compiles `fhir-types` through
  any crate.
- `eehrxf` 0.0.2: its `fhir-r4` feature compiles no openEHR crate (#730). The
  FHIRconnect mapping from openEHR moves behind a new `openehr` feature,
  which turns on `fhir-r4`, and the architecture test fails when `fhir-r4`
  alone reaches an openEHR crate or the mapping engine.
