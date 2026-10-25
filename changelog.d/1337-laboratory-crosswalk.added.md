- `eehrxf` 0.0.33 carries the laboratory report crosswalk under its
  `laboratory` feature: each Xt-EHR `EHDSLaboratoryReport` element with the
  eHealth Network ids of the Guideline on Laboratory Results Release 1.1,
  its producer obligation, and the element of the HL7 Europe Laboratory
  Report 2.0.0 composition or `DiagnosticReport` that carries it, checked
  against the vendored packages. A crosswalk names companion profiles beside
  its document profile, and `Crosswalk::check_with` holds its rows to all of
  them (#1337).
