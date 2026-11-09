- A laboratory or discharge report the FHIR face writes from a member's
  composition now passes its HL7 Europe document profile. Its `Patient`
  carries the name and birth date the demographics binding holds, and
  each one the binding does not hold is written with a `data-absent-reason`
  of `unknown` (HL7 Europe Base 2.0.1 `patient-eu-core`, `eu-pat-1`). The
  member the report was read at is a `Composition.author`, so the document
  rules of FHIR R4 admit its `Organization`. A demographics binding that
  cannot answer, or names no one patient, refuses the read with an
  `OperationOutcome` and no document is written (#1777).
