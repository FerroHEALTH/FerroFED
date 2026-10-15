- The interoperability component writes an openEHR terminology version as a
  FHIR `Coding.version` only in the form the code system's versions take
  (#977). A SNOMED CT version passes only as a URI naming an edition, such
  as `http://snomed.info/sct/900000000000207008/version/20260201`, and an
  HL7 Administrative Gender, RoleClass, RoleCode or ObservationCategoryCodes
  version only as dot-separated digits; any other is left out. Every other
  system's version passes as recorded, because no vendored source shows its
  form.
