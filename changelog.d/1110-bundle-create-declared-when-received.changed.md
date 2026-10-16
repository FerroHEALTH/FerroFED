- The capability statement of the FHIR face declares the `create` on
  `Bundle` only when a member receives a category, as it lists the
  prescription resources only when a member serves them (#1110); a
  deployment with no `[[fhir.receive]]` no longer declares a `create` that
  refuses every document.
