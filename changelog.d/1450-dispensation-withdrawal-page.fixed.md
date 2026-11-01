- The prescriptions page no longer describes a withdrawal of a dispensation
  as routed: the FHIR face offers none, and a dispensation sent again under
  the same identifier with `status` `entered-in-error` is refused `409`
  `duplicate` like any other, which a test now holds. The MyHealth@EU
  self-assessment row 07.04 says the same (#1450).
