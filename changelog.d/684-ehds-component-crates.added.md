- `crates/eehrxf` 0.0.1, the European interoperability component of
  Regulation (EU) 2025/327 as a library (#684). Its dataset model reads the
  Xt-EHR *EHDS Logical Information Models* package as data: every logical
  model and element path, with what a producer and a consumer must do with
  each element. One feature per Art 14(1) category names the dataset of that
  category. Behind `fhir-r4`, it compiles FHIRconnect mappings against an
  operational template and runs them in process over a canonical-JSON
  composition, through FerroBRIDGE's `fhirconnect` 0.1.108, answering a
  FHIR R4 Bundle with a `Provenance`. It depends on nothing in FerroFED and
  on no logging component. An architecture test fails when the two
  components reach each other or the application, when a crate other than
  the server links both, or when the engine reaches either. `fhir-types`
  moves to 0.1.108, the release `fhirconnect` builds on. `cargo deny` now
  checks every feature, so a dependency behind a feature no crate enables
  yet is checked too.
